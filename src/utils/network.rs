use rusty_network_manager::{
    dbus_interface_types::{NMDeviceType, NMState},
    AccessPointProxy, DeviceProxy, NetworkManagerProxy, WirelessProxy,
};
use std::{collections::HashMap, convert::TryFrom, fmt, str::from_utf8, sync::Arc};
use tokio::sync::Mutex;
use zbus::{
    zvariant::{ObjectPath, OwnedObjectPath, Value},
    Connection, Error as ZbusError,
};

const NM_STATE_CONNECTED_GLOBAL: u32 = 70;

#[derive(Debug, Clone)]
pub enum NetworkUtilError {
    Zbus(String),
    Nm(String),
    NoWifiDevice,
    Io(String),
    Utf8(std::str::Utf8Error),
    TypeConversion(String),
    InvalidEnumValue(String),
    TryFromIntError(std::num::TryFromIntError),
}

impl fmt::Display for NetworkUtilError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkUtilError::Zbus(e) => write!(f, "D-Bus error: {}", e),
            NetworkUtilError::Nm(s) => write!(f, "NetworkManager error: {}", s),
            NetworkUtilError::NoWifiDevice => write!(f, "No Wi-Fi device found"),
            NetworkUtilError::Io(e) => write!(f, "I/O error: {}", e),
            NetworkUtilError::Utf8(e) => write!(f, "UTF-8 conversion error: {}", e),
            NetworkUtilError::TypeConversion(s) => write!(f, "Type conversion error: {}", s),
            NetworkUtilError::InvalidEnumValue(s) => write!(f, "Invalid enum value: {}", s),
            NetworkUtilError::TryFromIntError(e) => write!(f, "Integer conversion error: {}", e),
        }
    }
}

impl std::error::Error for NetworkUtilError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NetworkUtilError::Utf8(e) => Some(e),
            NetworkUtilError::TryFromIntError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ZbusError> for NetworkUtilError {
    fn from(err: ZbusError) -> Self {
        NetworkUtilError::Zbus(err.to_string())
    }
}

impl From<std::io::Error> for NetworkUtilError {
    fn from(err: std::io::Error) -> Self {
        NetworkUtilError::Io(err.to_string())
    }
}

impl From<std::str::Utf8Error> for NetworkUtilError {
    fn from(err: std::str::Utf8Error) -> Self {
        NetworkUtilError::Utf8(err)
    }
}

impl From<zbus::zvariant::Error> for NetworkUtilError {
    fn from(err: zbus::zvariant::Error) -> Self {
        NetworkUtilError::TypeConversion(err.to_string())
    }
}

impl From<std::num::TryFromIntError> for NetworkUtilError {
    fn from(err: std::num::TryFromIntError) -> Self {
        NetworkUtilError::TryFromIntError(err)
    }
}

#[derive(Debug, Clone, Default)]
pub struct WifiDetails {
    pub enabled: bool,
    pub is_connected: bool,
    pub ssid: Option<String>,
    pub strength: Option<u8>,
    pub frequency: Option<u32>,
    pub bitrate: Option<u32>,
    pub icon_name: String,
    pub device_path: Option<OwnedObjectPath>,
}

#[derive(Debug, Clone)]
pub struct AccessPointInfo {
    pub path: OwnedObjectPath,
    pub ssid: Option<String>,
    pub strength: u8,
    pub icon_name: String,
    pub is_active: bool,
}

pub struct NetworkService {
    connection: Arc<Connection>,
    manager: NetworkManagerProxy<'static>,
    wifi_device_path: Mutex<Option<OwnedObjectPath>>,
}

impl NetworkService {
    pub async fn new() -> Result<Arc<Self>, NetworkUtilError> {
        let connection = Connection::system().await?;
        let connection = Arc::new(connection);
        let manager = NetworkManagerProxy::new(connection.as_ref())
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
        let service = Arc::new(Self {
            connection,
            manager,
            wifi_device_path: Mutex::new(None),
        });
        service.find_wifi_device().await?;
        Ok(service)
    }

    async fn find_wifi_device(&self) -> Result<(), NetworkUtilError> {
        let device_paths = self
            .manager
            .get_all_devices()
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
        let mut wifi_path: Option<OwnedObjectPath> = None;

        for path in device_paths {
            let device_proxy = DeviceProxy::new_from_path(path.clone(), self.connection.as_ref())
                .await
                .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
            let device_type_u32 = device_proxy
                .device_type()
                .await
                .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;

            match NMDeviceType::try_from(device_type_u32) {
                Ok(device_type_enum) => {
                    if device_type_enum == NMDeviceType::WIFI {
                        wifi_path = Some(path);
                        break;
                    }
                }
                Err(_) => {}
            }
        }

        *self.wifi_device_path.lock().await = wifi_path;
        Ok(())
    }

    async fn get_wifi_device_proxy<'a>(&'a self) -> Result<WirelessProxy<'a>, NetworkUtilError> {
        let path_guard = self.wifi_device_path.lock().await;
        let path = path_guard.as_ref().ok_or(NetworkUtilError::NoWifiDevice)?;
        let wireless_proxy = WirelessProxy::new_from_path(path.clone(), self.connection.as_ref())
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
        Ok(wireless_proxy)
    }

    pub async fn get_wifi_details(&self) -> Result<WifiDetails, NetworkUtilError> {
        let nm_state_u32 = self
            .manager
            .state()
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;

        let _flags: NMState = NMState::try_from(nm_state_u32).map_err(|_| {
            NetworkUtilError::InvalidEnumValue(format!("Invalid NMState value: {}", nm_state_u32))
        })?;

        let wifi_hw_enabled = self
            .manager
            .wireless_hardware_enabled()
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
        let wifi_enabled = self
            .manager
            .wireless_enabled()
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;

        if !wifi_hw_enabled || !wifi_enabled {
            return Ok(WifiDetails {
                enabled: false,
                icon_name: "network-wireless-disabled-symbolic".to_string(),
                ..Default::default()
            });
        }

        let wifi_proxy = match self.get_wifi_device_proxy().await {
            Ok(proxy) => proxy,
            Err(NetworkUtilError::NoWifiDevice) => {
                return Ok(WifiDetails {
                    enabled: true,
                    icon_name: "network-wireless-offline-symbolic".to_string(),
                    ..Default::default()
                });
            }
            Err(e) => return Err(e),
        };

        let is_connected = nm_state_u32 >= NM_STATE_CONNECTED_GLOBAL;
        let active_ap_path = wifi_proxy
            .active_access_point()
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))
            .ok();
        let device_path = self.wifi_device_path.lock().await.clone();

        let mut details = WifiDetails {
            enabled: true,
            is_connected,
            device_path,
            ..Default::default()
        };

        if let Some(ref ap_path) = active_ap_path {
            if !ap_path.as_str().eq("/") {
                let ap_proxy =
                    AccessPointProxy::new_from_path(ap_path.clone(), self.connection.as_ref())
                        .await
                        .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
                let ssid_bytes = ap_proxy
                    .ssid()
                    .await
                    .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
                details.ssid = Some(from_utf8(&ssid_bytes)?.to_string());
                details.strength = Some(
                    ap_proxy
                        .strength()
                        .await
                        .map_err(|e| NetworkUtilError::Nm(e.to_string()))?,
                );
                details.frequency = Some(
                    ap_proxy
                        .frequency()
                        .await
                        .map_err(|e| NetworkUtilError::Nm(e.to_string()))?,
                );
                details.bitrate = Some(
                    ap_proxy
                        .max_bitrate()
                        .await
                        .map_err(|e| NetworkUtilError::Nm(e.to_string()))?,
                );
            } else {
                details.is_connected = false;
            }
        } else {
            details.is_connected = false;
        }

        details.icon_name = get_wifi_icon_name(details.is_connected, details.strength);

        Ok(details)
    }

    pub async fn set_wifi_enabled(&self, enabled: bool) -> Result<(), NetworkUtilError> {
        let proxy = zbus::Proxy::new(
            self.connection.as_ref(),
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.NetworkManager",
        )
        .await?;
        proxy
            .set_property("WirelessEnabled", Value::from(enabled))
            .await
            .map_err(|e| NetworkUtilError::Zbus(e.to_string()))?;

        if enabled {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            self.find_wifi_device().await?;
        } else {
            *self.wifi_device_path.lock().await = None;
        }
        Ok(())
    }

    pub async fn request_scan(&self) -> Result<(), NetworkUtilError> {
        let wifi_proxy = self.get_wifi_device_proxy().await?;
        wifi_proxy
            .request_scan(HashMap::new())
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
        Ok(())
    }

    pub async fn get_access_points(&self) -> Result<Vec<AccessPointInfo>, NetworkUtilError> {
        let wifi_proxy = self.get_wifi_device_proxy().await?;
        let ap_paths = wifi_proxy
            .get_access_points()
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
        let active_ap_path = wifi_proxy
            .active_access_point()
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))
            .ok();

        let mut ap_infos = Vec::new();
        for ap_path in ap_paths {
            let ap_proxy =
                AccessPointProxy::new_from_path(ap_path.clone(), self.connection.as_ref())
                    .await
                    .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
            let strength = ap_proxy
                .strength()
                .await
                .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
            let ssid_bytes = ap_proxy
                .ssid()
                .await
                .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
            let ssid = from_utf8(&ssid_bytes).ok().map(String::from);

            let is_active = active_ap_path.as_ref() == Some(&ap_path);

            let info = AccessPointInfo {
                path: ap_path,
                ssid,
                strength,
                icon_name: get_wifi_signal_icon_name(strength),
                is_active,
            };
            ap_infos.push(info);
        }

        ap_infos.sort_unstable_by(|a, b| {
            b.is_active
                .cmp(&a.is_active)
                .then_with(|| b.strength.cmp(&a.strength))
                .then_with(|| a.ssid.cmp(&b.ssid))
        });

        Ok(ap_infos)
    }

    pub async fn connect_to_network(
        &self,
        ap_path: &OwnedObjectPath,
        device_path: &OwnedObjectPath,
    ) -> Result<(), NetworkUtilError> {
        let root_path = ObjectPath::try_from("/")?;
        let device_obj_path = ObjectPath::try_from(device_path.as_str())?;
        let ap_obj_path = ObjectPath::try_from(ap_path.as_str())?;

        self.manager
            .activate_connection(&root_path, &device_obj_path, &ap_obj_path)
            .await
            .map_err(|e| NetworkUtilError::Nm(e.to_string()))?;
        Ok(())
    }

    pub async fn get_airplane_mode_state(&self) -> Result<bool, NetworkUtilError> {
        let proxy = zbus::Proxy::new(
            self.connection.as_ref(),
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.NetworkManager",
        )
        .await?;

        let wireless_enabled: bool = proxy.get_property("WirelessEnabled").await?;
        let wwan_enabled: bool = proxy.get_property("WwanEnabled").await?;

        Ok(!wireless_enabled && !wwan_enabled)
    }

    pub async fn set_airplane_mode(&self, enabled: bool) -> Result<(), NetworkUtilError> {
        let proxy = zbus::Proxy::new(
            self.connection.as_ref(),
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.NetworkManager",
        )
        .await?;

        let wireless_enabled = proxy
            .set_property("WirelessEnabled", Value::from(!enabled))
            .await;
        let wwan_enabled = proxy
            .set_property("WwanEnabled", Value::from(!enabled))
            .await;

        wireless_enabled.map_err(|e| NetworkUtilError::Zbus(e.to_string()))?;
        wwan_enabled.map_err(|e| NetworkUtilError::Zbus(e.to_string()))?;

        Ok(())
    }
}

fn get_wifi_icon_name(connected: bool, strength: Option<u8>) -> String {
    if !connected {
        return "network-wireless-offline-symbolic".to_string();
    }
    match strength {
        Some(s) => get_wifi_signal_icon_name(s),
        None => "network-wireless-signal-none-symbolic".to_string(),
    }
}

fn get_wifi_signal_icon_name(strength: u8) -> String {
    match strength {
        80..=100 => "network-wireless-signal-excellent-symbolic",
        60..=79 => "network-wireless-signal-good-symbolic",
        40..=59 => "network-wireless-signal-ok-symbolic",
        20..=39 => "network-wireless-signal-weak-symbolic",
        _ => "network-wireless-signal-none-symbolic",
    }
    .to_string()
}

#[derive(Debug)]
pub enum NetworkCommand {
    GetDetails,
    GetAccessPoints,
    RequestScan,
    SetWifiEnabled(bool),
    SetAirplaneMode(bool),
    GetAirplaneModeState,
    ConnectToNetwork {
        ap_path: OwnedObjectPath,
        device_path: OwnedObjectPath,
    },
}

#[derive(Debug, Clone)]
pub enum NetworkResult {
    Details(Result<WifiDetails, NetworkUtilError>),
    AccessPoints(Result<Vec<AccessPointInfo>, NetworkUtilError>),
    ScanRequested(Result<(), NetworkUtilError>),
    WifiSet(Result<(), NetworkUtilError>),
    AirplaneModeSet(Result<(), NetworkUtilError>),
    AirplaneModeState(Result<bool, NetworkUtilError>),
    Connected(Result<(), NetworkUtilError>),
}
