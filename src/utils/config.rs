use serde::{Deserialize, Serialize};
use std::{error::Error, fs, path::PathBuf};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleType {
    AppMenu,
    ActiveClient,
    Clock,
    Battery,
    Network,
    Display,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum DockPosition {
    Left,
    Right,
    Bottom,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct BatteryConfig {
    pub show_details: bool,
    pub show_power_profiles: bool,
    pub show_conservation_mode: bool,
    #[serde(rename = "conservation-mode-path")]
    pub conservation_mode_path: Option<PathBuf>,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            show_details: true,
            show_power_profiles: true,
            show_conservation_mode: true,
            conservation_mode_path: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct NetworkConfig {}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct DockConfig {
    pub position: DockPosition,
    pub icon_size: u32,
    pub auto_hide: bool,
    pub show_running_apps: bool,
    pub show_favorites: bool,
    pub favorites: Vec<String>,
    pub reveal_delay: u32,
    pub hide_delay: u32,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            position: DockPosition::Bottom,
            icon_size: 48,
            auto_hide: false,
            show_running_apps: true,
            show_favorites: true,
            favorites: vec![
                "zen".to_string(),
                "firefox".to_string(),
                "nautilus".to_string(),
                "terminal".to_string(),
            ],
            reveal_delay: 200,
            hide_delay: 1000,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct BarConfig {
    pub font: Option<String>,
    pub modules_left: Vec<ModuleType>,
    pub modules_center: Vec<ModuleType>,
    pub modules_right: Vec<ModuleType>,
    pub distro_icon_override: Option<String>,
    pub clock_format: Option<String>,
    pub notification_position: NotificationPosition,
    #[serde(rename = "active-client-max-length")]
    pub active_client_max_length: usize,
    pub battery: BatteryConfig,
    pub network: NetworkConfig,
    pub dock: DockConfig,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            font: Some("Sans 10".to_string()),
            modules_left: vec![ModuleType::AppMenu, ModuleType::ActiveClient],
            modules_center: vec![],
            modules_right: vec![ModuleType::Battery, ModuleType::Clock],
            distro_icon_override: None,
            clock_format: Some("%A %e, %H:%M".to_string()),
            notification_position: NotificationPosition::TopRight,
            active_client_max_length: 20,
            battery: BatteryConfig::default(),
            network: NetworkConfig::default(),
            dock: DockConfig::default(),
        }
    }
}

fn get_config_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut path = dirs::config_dir().ok_or("Could not determine config directory")?;
    path.push("kaneru");
    path.push("config.yaml");
    Ok(path)
}

fn ensure_config_exists() -> Result<(), Box<dyn Error>> {
    let config_path = get_config_path()?;
    if !config_path.exists() {
        let config_dir = config_path.parent().ok_or("No parent")?;
        fs::create_dir_all(config_dir)?;
        let default = BarConfig::default();
        let yaml = serde_yaml::to_string(&default)?;
        fs::write(&config_path, yaml)?;
    }
    Ok(())
}

pub fn load_config() -> BarConfig {
    fn load() -> Result<BarConfig, Box<dyn Error>> {
        ensure_config_exists()?;
        let path = get_config_path()?;
        let s = fs::read_to_string(&path)?;
        let cfg: BarConfig = serde_yaml::from_str(&s)?;
        Ok(cfg)
    }
    load().unwrap_or_else(|e| {
        eprintln!("Failed to load config: {}. Using default.", e);
        BarConfig::default()
    })
}
