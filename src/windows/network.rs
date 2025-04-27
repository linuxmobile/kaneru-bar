use crate::utils::{
    config::NetworkConfig,
    network::{AccessPointInfo, NetworkService, NetworkUtilError, WifiDetails},
};
use gtk4::prelude::*;
use gtk4::{
    glib::{self},
    Align, Box as GtkBox, Button, Image, Label, Orientation, PolicyType, Popover, Revealer,
    RevealerTransitionType, ScrolledWindow, Separator, Spinner, Switch,
};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use zbus::zvariant::{OwnedObjectPath, Value};

const REFRESH_INTERVAL_WINDOW: Duration = Duration::from_secs(10);
const SCAN_INTERVAL: Duration = Duration::from_secs(20);
const SCAN_RESULT_DELAY: Duration = Duration::from_secs(2);

struct NetworkWindowUI {
    main_box: GtkBox,
    airplane_toggle_button: Button,
    airplane_switch: Switch,
    wifi_toggle_button: Button,
    wifi_switch: Switch,
    current_icon: Image,
    current_ssid_label: Label,
    current_details_box: GtkBox,
    strength_label: Label,
    frequency_label: Label,
    bandwidth_label: Label,
    networks_revealer: Revealer,
    networks_list_box: GtkBox,
    scan_spinner: Spinner,
    scan_status_label: Label,
    available_networks_button_icon: Image,
}

pub struct NetworkWindow {
    popover: Popover,
    _config: NetworkConfig,
    service: Arc<NetworkService>,
    details: Arc<Mutex<Option<WifiDetails>>>,
    access_points: Arc<Mutex<Vec<AccessPointInfo>>>,
    ui_elements: Rc<RefCell<Option<NetworkWindowUI>>>,
    is_scanning: Rc<RefCell<bool>>,
    pub networks_visible: Rc<RefCell<bool>>,
    polling_active: Rc<RefCell<bool>>,
    airplane_mode_active: Rc<RefCell<bool>>,
    update_source_id: Rc<RefCell<Option<glib::SourceId>>>,
    scan_source_id: Rc<RefCell<Option<glib::SourceId>>>,
}

impl NetworkWindow {
    pub fn new(config: &NetworkConfig, service: Arc<NetworkService>) -> Rc<Self> {
        let popover = Popover::builder()
            .autohide(true)
            .cascade_popdown(true)
            .build();
        popover.add_css_class("NetworkWindow");

        let details = Arc::new(Mutex::new(None));
        let access_points = Arc::new(Mutex::new(Vec::new()));
        let ui_elements = Rc::new(RefCell::new(None));
        let update_source_id = Rc::new(RefCell::new(None));
        let scan_source_id = Rc::new(RefCell::new(None));
        let is_scanning = Rc::new(RefCell::new(false));
        let networks_visible = Rc::new(RefCell::new(false));
        let polling_active = Rc::new(RefCell::new(false));
        let airplane_mode_active = Rc::new(RefCell::new(false));

        let window = Rc::new(Self {
            popover: popover.clone(),
            _config: config.clone(),
            service,
            details: details.clone(),
            access_points: access_points.clone(),
            ui_elements: ui_elements.clone(),
            is_scanning: is_scanning.clone(),
            networks_visible: networks_visible.clone(),
            polling_active: polling_active.clone(),
            airplane_mode_active: airplane_mode_active.clone(),
            update_source_id: update_source_id.clone(),
            scan_source_id: scan_source_id.clone(),
        });

        let main_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .width_request(350)
            .build();

        let (top_bar, airplane_toggle_button, airplane_switch, wifi_toggle_button, wifi_switch) =
            Self::build_top_bar(window.clone());
        main_box.append(&top_bar);
        main_box.append(&Separator::new(Orientation::Horizontal));

        let (
            current_section,
            _current_network_box,
            current_icon,
            current_ssid_label,
            current_details_box,
            strength_label,
            frequency_label,
            bandwidth_label,
        ) = Self::build_current_network_section();
        main_box.append(&current_section);
        main_box.append(&Separator::new(Orientation::Horizontal));

        let (
            available_section,
            networks_revealer,
            networks_list_box,
            scan_spinner,
            scan_status_label,
            available_networks_button_icon,
        ) = Self::build_available_networks_section(window.clone());
        main_box.append(&available_section);
        main_box.append(&Separator::new(Orientation::Horizontal));

        let settings_section = Self::build_settings_section(popover.clone());
        main_box.append(&settings_section);

        popover.set_child(Some(&main_box));

        *ui_elements.borrow_mut() = Some(NetworkWindowUI {
            main_box,
            airplane_toggle_button,
            airplane_switch,
            wifi_toggle_button,
            wifi_switch,
            current_icon,
            current_ssid_label,
            current_details_box,
            strength_label,
            frequency_label,
            bandwidth_label,
            networks_revealer,
            networks_list_box,
            scan_spinner,
            scan_status_label,
            available_networks_button_icon,
        });

        let window_clone = window.clone();
        popover.connect_visible_notify(move |pop| {
            if pop.is_visible() {
                window_clone.start_polling();
            } else {
                window_clone.stop_polling();
                if *window_clone.networks_visible.borrow() {
                    if let Some(ui) = window_clone.ui_elements.borrow().as_ref() {
                        ui.networks_revealer.set_reveal_child(false);
                        ui.available_networks_button_icon
                            .remove_css_class("expanded");
                    }
                    *window_clone.networks_visible.borrow_mut() = false;
                }
            }
        });

        window
    }

    fn build_quick_toggle_button(icon_name: &str, label_text: &str) -> Button {
        let icon = Image::builder()
            .icon_name(icon_name)
            .pixel_size(20)
            .margin_bottom(3)
            .halign(Align::Center)
            .build();
        icon.add_css_class("toggle-icon");

        let label = Label::builder()
            .label(label_text)
            .halign(Align::Center)
            .build();
        label.add_css_class("toggle-label");

        let content_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(2)
            .valign(Align::Center)
            .build();
        content_box.append(&icon);
        content_box.append(&label);

        let button = Button::builder()
            .child(&content_box)
            .css_classes(vec!["quick-toggle"])
            .hexpand(true)
            .build();

        button
    }

    fn build_top_bar(window_rc: Rc<Self>) -> (GtkBox, Button, Switch, Button, Switch) {
        let wifi_switch = Switch::builder().visible(false).build();
        let wifi_toggle_button =
            Self::build_quick_toggle_button("network-wireless-symbolic", "Wi-Fi");

        let airplane_switch = Switch::builder().visible(false).build();
        let airplane_toggle_button =
            Self::build_quick_toggle_button("airplane-mode-symbolic", "Airplane");

        let toggles_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .halign(Align::Fill)
            .hexpand(true)
            .build();
        toggles_box.append(&airplane_toggle_button);
        toggles_box.append(&wifi_toggle_button);

        let top_bar = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .css_classes(vec!["quick-settings-row"])
            .build();
        top_bar.append(&toggles_box);

        let weak_window = Rc::downgrade(&window_rc);
        let wifi_switch_clone = wifi_switch.clone();
        wifi_toggle_button.connect_clicked(move |_| {
            let current_state = wifi_switch_clone.state();
            wifi_switch_clone.set_state(!current_state);
        });

        wifi_switch.connect_state_set(move |_, state| {
            if let Some(window) = weak_window.upgrade() {
                let service = window.service.clone();
                window.set_controls_sensitive(false);
                let weak_window_async = weak_window.clone();
                glib::MainContext::default().spawn_local(async move {
                    match service.set_wifi_enabled(state).await {
                        Ok(_) => {
                            if let Some(w) = weak_window_async.upgrade() {
                                w.update_all_data().await;
                                w.set_controls_sensitive(true);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to set Wi-Fi state: {}", e);
                            if let Some(w) = weak_window_async.upgrade() {
                                w.update_switches_state().await;
                                w.set_controls_sensitive(true);
                            }
                        }
                    }
                });
            }
            glib::Propagation::Stop
        });

        let weak_window_ap = Rc::downgrade(&window_rc);
        let airplane_switch_clone = airplane_switch.clone();
        airplane_toggle_button.connect_clicked(move |_| {
            let current_state = airplane_switch_clone.state();
            airplane_switch_clone.set_state(!current_state);
        });

        airplane_switch.connect_state_set(move |_, state| {
            if let Some(window) = weak_window_ap.upgrade() {
                window.set_controls_sensitive(false);
                let weak_window_async = weak_window_ap.clone();
                glib::MainContext::default().spawn_local(async move {
                    match Self::set_airplane_mode(state).await {
                        Ok(_) => {
                            if let Some(w) = weak_window_async.upgrade() {
                                *w.airplane_mode_active.borrow_mut() = state;
                                w.update_all_data().await;
                                w.set_controls_sensitive(true);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to set Airplane Mode state: {}", e);
                            if let Some(w) = weak_window_async.upgrade() {
                                w.update_switches_state().await;
                                w.set_controls_sensitive(true);
                            }
                        }
                    }
                });
            }
            glib::Propagation::Stop
        });

        (
            top_bar,
            airplane_toggle_button,
            airplane_switch,
            wifi_toggle_button,
            wifi_switch,
        )
    }

    async fn set_airplane_mode(enabled: bool) -> Result<(), NetworkUtilError> {
        let connection = zbus::Connection::system().await?;
        let proxy = zbus::Proxy::new(
            &connection,
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

        wireless_enabled.map_err(|e| NetworkUtilError::Zbus(zbus::Error::FDO(Box::new(e))))?;
        wwan_enabled.map_err(|e| NetworkUtilError::Zbus(zbus::Error::FDO(Box::new(e))))?;

        Ok(())
    }

    async fn get_airplane_mode_state() -> Result<bool, NetworkUtilError> {
        let connection = zbus::Connection::system().await?;
        let proxy = zbus::Proxy::new(
            &connection,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.NetworkManager",
        )
        .await?;

        let wireless_enabled: bool = proxy.get_property("WirelessEnabled").await?;
        let wwan_enabled: bool = proxy.get_property("WwanEnabled").await?;

        Ok(!wireless_enabled && !wwan_enabled)
    }

    fn build_current_network_section() -> (GtkBox, GtkBox, Image, Label, GtkBox, Label, Label, Label)
    {
        let current_icon = Image::builder()
            .icon_name("network-wireless-offline-symbolic")
            .pixel_size(24)
            .build();

        let current_ssid_label = Label::builder()
            .label("Not Connected")
            .halign(Align::Start)
            .hexpand(true)
            .css_classes(vec!["title-3"])
            .build();

        let current_network_info_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .build();
        current_network_info_box.append(&current_icon);
        current_network_info_box.append(&current_ssid_label);

        let (strength_row, strength_label) = Self::create_detail_row("Signal Strength:");
        let (frequency_row, frequency_label) = Self::create_detail_row("Frequency:");
        let (bandwidth_row, bandwidth_label) = Self::create_detail_row("Bandwidth:");

        let current_details_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .visible(false)
            .css_classes(vec!["network-details"])
            .build();
        current_details_box.append(&strength_row);
        current_details_box.append(&frequency_row);
        current_details_box.append(&bandwidth_row);

        let current_section = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(5)
            .css_classes(vec!["current-network"])
            .build();
        current_section.append(&current_network_info_box);
        current_section.append(&current_details_box);

        (
            current_section,
            current_network_info_box,
            current_icon,
            current_ssid_label,
            current_details_box,
            strength_label,
            frequency_label,
            bandwidth_label,
        )
    }

    fn build_available_networks_section(
        window_rc: Rc<Self>,
    ) -> (GtkBox, Revealer, GtkBox, Spinner, Label, Image) {
        let scan_spinner = Spinner::builder().spinning(false).visible(false).build();

        let scan_status_label = Label::builder()
            .label("Available Networks")
            .halign(Align::Start)
            .hexpand(true)
            .build();

        let available_networks_button_icon =
            Image::builder().icon_name("pan-down-symbolic").build();

        let button_content = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .build();
        button_content.append(&scan_status_label);
        button_content.append(&scan_spinner);
        button_content.append(&available_networks_button_icon);

        let available_networks_button = Button::builder()
            .child(&button_content)
            .css_classes(vec!["network-selector"])
            .build();

        let networks_list_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(5)
            .css_classes(vec!["network-list"])
            .build();

        let scrolled_window = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .min_content_height(150)
            .max_content_height(250)
            .min_content_width(300)
            .child(&networks_list_box)
            .build();

        let networks_revealer = Revealer::builder()
            .transition_type(RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .child(&scrolled_window)
            .reveal_child(false)
            .build();

        let weak_window = Rc::downgrade(&window_rc);
        let revealer_clone = networks_revealer.clone();
        let icon_clone = available_networks_button_icon.clone();
        available_networks_button.connect_clicked(move |_| {
            if let Some(window) = weak_window.upgrade() {
                let should_reveal = !revealer_clone.reveals_child();
                revealer_clone.set_reveal_child(should_reveal);
                *window.networks_visible.borrow_mut() = should_reveal;

                if should_reveal {
                    icon_clone.add_css_class("expanded");
                } else {
                    icon_clone.remove_css_class("expanded");
                }

                if should_reveal {
                    window.trigger_scan();
                    window.start_scan_timer();
                } else {
                    window.stop_scan_timer();
                    window.set_scanning_state(false);
                }
            }
        });

        let networks_container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .css_classes(vec!["networks-container"])
            .build();
        networks_container.append(&available_networks_button);
        networks_container.append(&networks_revealer);

        let available_section = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .css_classes(vec!["networks-section"])
            .build();
        available_section.append(&networks_container);

        (
            available_section,
            networks_revealer,
            networks_list_box,
            scan_spinner,
            scan_status_label,
            available_networks_button_icon,
        )
    }

    fn build_settings_section(popover: Popover) -> GtkBox {
        let button = Button::builder().label("Network Settings").build();

        let popover_clone = popover.clone();
        button.connect_clicked(move |_| {
            popover_clone.popdown();
            let _ = std::process::Command::new("env")
                .args(["XDG_CURRENT_DESKTOP=GNOME", "gnome-control-center", "wifi"])
                .spawn();
        });

        let section_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(vec!["settings"])
            .halign(Align::Fill)
            .hexpand(true)
            .build();
        section_box.append(&button);
        section_box
    }

    fn create_detail_row(label_text: &str) -> (GtkBox, Label) {
        let label = Label::builder()
            .label(label_text)
            .halign(Align::Start)
            .build();
        let value_label = Label::builder()
            .label("N/A")
            .halign(Align::End)
            .hexpand(true)
            .css_classes(vec!["dim-label"])
            .build();
        let row_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .build();
        row_box.append(&label);
        row_box.append(&value_label);
        (row_box, value_label)
    }

    async fn update_all_data(self: &Rc<Self>) {
        let details_res = self.service.get_wifi_details().await;
        match details_res {
            Ok(d) => {
                let mut details_guard = self.details.lock().await;
                *details_guard = Some(d);
            }
            Err(e) => {
                eprintln!("Failed to get network details: {}", e);
                let mut details_guard = self.details.lock().await;
                *details_guard = None;
            }
        }

        match Self::get_airplane_mode_state().await {
            Ok(state) => *self.airplane_mode_active.borrow_mut() = state,
            Err(e) => {
                eprintln!("Failed to get airplane mode state: {}", e);
            }
        }

        self.update_ui().await;
    }

    async fn update_ui(self: &Rc<Self>) {
        let ui_opt = self.ui_elements.borrow();
        let ui = match ui_opt.as_ref() {
            Some(ui) => ui,
            None => return,
        };
        let details_guard = self.details.lock().await;
        let details = details_guard.as_ref();
        let airplane_mode = *self.airplane_mode_active.borrow();

        let is_wifi_enabled = details.map_or(false, |d| d.enabled);

        ui.wifi_switch.set_state(is_wifi_enabled);
        ui.airplane_switch.set_state(airplane_mode);

        if is_wifi_enabled {
            ui.wifi_toggle_button.add_css_class("active");
        } else {
            ui.wifi_toggle_button.remove_css_class("active");
        }
        if airplane_mode {
            ui.airplane_toggle_button.add_css_class("active");
        } else {
            ui.airplane_toggle_button.remove_css_class("active");
        }

        let controls_sensitive = !airplane_mode;
        ui.main_box.set_sensitive(controls_sensitive);
        ui.wifi_toggle_button.set_sensitive(controls_sensitive);
        ui.wifi_switch.set_sensitive(controls_sensitive);
        ui.airplane_toggle_button.set_sensitive(true);
        ui.airplane_switch.set_sensitive(true);

        if let Some(d) = details {
            if airplane_mode {
                ui.current_icon
                    .set_icon_name(Some("airplane-mode-symbolic"));
                ui.current_ssid_label.set_label("Airplane Mode");
                ui.current_details_box.set_visible(false);
            } else {
                ui.current_icon.set_icon_name(Some(&d.icon_name));
                if d.is_connected {
                    ui.current_ssid_label
                        .set_label(d.ssid.as_deref().unwrap_or("Connected"));
                    ui.current_details_box.set_visible(true);
                    ui.strength_label
                        .set_label(&format!("{}%", d.strength.unwrap_or(0)));
                    ui.frequency_label.set_label(&format!(
                        "{:.1} GHz",
                        d.frequency.unwrap_or(0) as f32 / 1000.0
                    ));
                    ui.bandwidth_label
                        .set_label(&format!("{} Mbps", d.bitrate.unwrap_or(0) / 1000));
                } else {
                    ui.current_ssid_label.set_label(if d.enabled {
                        "Not Connected"
                    } else {
                        "Wi-Fi Disabled"
                    });
                    ui.current_details_box.set_visible(false);
                }
            }
        } else {
            if airplane_mode {
                ui.current_icon
                    .set_icon_name(Some("airplane-mode-symbolic"));
                ui.current_ssid_label.set_label("Airplane Mode");
            } else {
                ui.current_icon
                    .set_icon_name(Some("network-wireless-offline-symbolic"));
                ui.current_ssid_label.set_label("N/A");
            }
            ui.current_details_box.set_visible(false);
        }
    }

    async fn update_switches_state(self: &Rc<Self>) {
        if let Some(ui) = self.ui_elements.borrow().as_ref() {
            let details_guard = self.details.lock().await;
            let is_wifi_enabled = details_guard.as_ref().map_or(false, |d| d.enabled);
            ui.wifi_switch.set_state(is_wifi_enabled);
            if is_wifi_enabled {
                ui.wifi_toggle_button.add_css_class("active");
            } else {
                ui.wifi_toggle_button.remove_css_class("active");
            }

            match Self::get_airplane_mode_state().await {
                Ok(state) => {
                    *self.airplane_mode_active.borrow_mut() = state;
                    ui.airplane_switch.set_state(state);
                    if state {
                        ui.airplane_toggle_button.add_css_class("active");
                    } else {
                        ui.airplane_toggle_button.remove_css_class("active");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to get airplane mode state for switch update: {}", e);
                }
            }
        }
    }

    fn set_controls_sensitive(self: &Rc<Self>, sensitive: bool) {
        if let Some(ui) = self.ui_elements.borrow().as_ref() {
            let airplane_mode = *self.airplane_mode_active.borrow();
            let overall_sensitive = sensitive && !airplane_mode;

            ui.main_box.set_sensitive(overall_sensitive);

            ui.wifi_toggle_button.set_sensitive(overall_sensitive);
            ui.wifi_switch.set_sensitive(overall_sensitive);

            ui.airplane_toggle_button.set_sensitive(sensitive);
            ui.airplane_switch.set_sensitive(sensitive);

            if let Some(available_networks_button) = ui
                .networks_revealer
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.first_child())
                .and_then(|w| w.downcast::<Button>().ok())
            {
                available_networks_button.set_sensitive(overall_sensitive);
            }
            ui.networks_list_box.set_sensitive(overall_sensitive);
        }
    }

    async fn update_access_points_list(self: &Rc<Self>) {
        if !*self.networks_visible.borrow() {
            self.set_scanning_state(false);
            return;
        }
        self.set_scanning_state(true);
        match self.service.get_access_points().await {
            Ok(aps) => {
                let mut ap_guard = self.access_points.lock().await;
                *ap_guard = aps;
                drop(ap_guard);
                self.rebuild_network_list_ui().await;
            }
            Err(e) => {
                eprintln!("[Scan] Failed to get access points: {}", e);
                let mut ap_guard = self.access_points.lock().await;
                ap_guard.clear();
                drop(ap_guard);
                self.rebuild_network_list_ui().await;
            }
        }
        self.set_scanning_state(false);
    }

    async fn rebuild_network_list_ui(self: &Rc<Self>) {
        let weak_self = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(s) = weak_self.upgrade() {
                if let Some(ui_inner) = s.ui_elements.borrow().as_ref() {
                    while let Some(child) = ui_inner.networks_list_box.first_child() {
                        ui_inner.networks_list_box.remove(&child);
                    }

                    let ap_guard = match s.access_points.try_lock() {
                        Ok(guard) => guard,
                        Err(_) => return,
                    };

                    if ap_guard.is_empty() {
                        let msg = if *s.is_scanning.borrow() {
                            "Scanning..."
                        } else {
                            "No networks found"
                        };
                        let label = Label::builder()
                            .label(msg)
                            .halign(Align::Center)
                            .css_classes(vec!["dim-label"])
                            .margin_top(20)
                            .margin_bottom(20)
                            .build();
                        ui_inner.networks_list_box.append(&label);
                        return;
                    }

                    let details_guard = match s.details.try_lock() {
                        Ok(guard) => guard,
                        Err(_) => return,
                    };
                    let device_path = details_guard.as_ref().and_then(|d| d.device_path.clone());
                    drop(details_guard);

                    for ap in ap_guard.iter() {
                        let ap_button = s.create_ap_button(ap, device_path.as_ref());
                        ui_inner.networks_list_box.append(&ap_button);
                    }
                }
            }
        });
    }

    fn create_ap_button(
        self: &Rc<Self>,
        ap: &AccessPointInfo,
        device_path: Option<&OwnedObjectPath>,
    ) -> Button {
        let icon = Image::builder().icon_name(&ap.icon_name).build();
        let ssid_label = Label::builder()
            .label(ap.ssid.as_deref().unwrap_or("Hidden Network"))
            .halign(Align::Start)
            .hexpand(true)
            .build();

        let content = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .build();
        content.append(&icon);
        content.append(&ssid_label);

        if ap.is_active {
            let check_icon = Image::builder().icon_name("emblem-ok-symbolic").build();
            content.append(&check_icon);
        }

        let button = Button::builder()
            .child(&content)
            .css_classes(vec!["network-item", "flat"])
            .sensitive(!ap.is_active)
            .build();

        if ap.is_active {
            button.add_css_class("active");
        }

        let weak_self = Rc::downgrade(self);
        let ap_path = ap.path.clone();
        let device_path_clone = device_path.cloned();

        button.connect_clicked(move |btn| {
            if let Some(window) = weak_self.upgrade() {
                if let Some(dev_path) = &device_path_clone {
                    btn.set_sensitive(false);
                    let service = window.service.clone();
                    let ap_path_clone = ap_path.clone();
                    let dev_path_clone = dev_path.clone();
                    let weak_btn = btn.downgrade();
                    let weak_self_async = weak_self.clone();

                    glib::MainContext::default().spawn_local(async move {
                        println!(
                            "Attempting connection to AP: {:?}, Device: {:?}",
                            ap_path_clone, dev_path_clone
                        );
                        match service
                            .connect_to_network(&ap_path_clone, &dev_path_clone)
                            .await
                        {
                            Ok(_) => {
                                println!("Connection initiated (may require agent)");
                            }
                            Err(e) => {
                                eprintln!("Failed to connect: {}", e);
                                if let Some(b) = weak_btn.upgrade() {
                                    b.set_sensitive(true);
                                }
                            }
                        }
                        if let Some(w) = weak_self_async.upgrade() {
                            w.update_all_data().await;
                        }
                    });
                } else {
                    eprintln!("Cannot connect: Wi-Fi device path unknown.");
                }
            }
        });

        button
    }

    fn set_scanning_state(self: &Rc<Self>, scanning: bool) {
        *self.is_scanning.borrow_mut() = scanning;
        if let Some(ui) = self.ui_elements.borrow().as_ref() {
            ui.scan_spinner.set_visible(scanning);
            if scanning {
                ui.scan_spinner.start();
                ui.scan_status_label.set_label("Scanning...");
            } else {
                ui.scan_spinner.stop();
                ui.scan_status_label.set_label("Available Networks");
            }
            if scanning && ui.networks_list_box.first_child().is_none() {
                let weak_self = Rc::downgrade(self);
                glib::idle_add_local_once(move || {
                    if let Some(s) = weak_self.upgrade() {
                        let _ = s.rebuild_network_list_ui();
                    }
                });
            }
        }
    }

    fn trigger_scan(self: &Rc<Self>) {
        if !*self.networks_visible.borrow() {
            return;
        }
        if *self.is_scanning.borrow() {
            return;
        }

        let weak_self = Rc::downgrade(self);
        let service = self.service.clone();
        self.set_scanning_state(true);

        glib::MainContext::default().spawn_local(async move {
            match service.request_scan().await {
                Ok(_) => {
                    tokio::time::sleep(SCAN_RESULT_DELAY).await;
                    if let Some(s) = weak_self.upgrade() {
                        if *s.networks_visible.borrow() {
                            s.update_access_points_list().await;
                        } else {
                            s.set_scanning_state(false);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[Scan] Failed to request scan: {}", e);
                    if let Some(s) = weak_self.upgrade() {
                        s.set_scanning_state(false);
                    }
                }
            }
        });
    }

    fn start_polling(self: &Rc<Self>) {
        if *self.polling_active.borrow() {
            return;
        }
        *self.polling_active.borrow_mut() = true;

        let weak_self_init = Rc::downgrade(self);
        glib::MainContext::default().spawn_local(async move {
            if let Some(s) = weak_self_init.upgrade() {
                s.update_all_data().await;
            }
        });

        let weak_self = Rc::downgrade(self);
        let id = glib::timeout_add_local(REFRESH_INTERVAL_WINDOW, move || {
            if let Some(inner_self) = weak_self.upgrade() {
                if !inner_self.popover.is_visible() {
                    *inner_self.polling_active.borrow_mut() = false;
                    *inner_self.update_source_id.borrow_mut() = None;
                    return glib::ControlFlow::Break;
                }
                let s_clone = inner_self.clone();
                glib::MainContext::default().spawn_local(async move {
                    s_clone.update_all_data().await;
                });
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        *self.update_source_id.borrow_mut() = Some(id);

        if *self.networks_visible.borrow() {
            self.start_scan_timer();
        }
    }

    fn stop_polling(self: &Rc<Self>) {
        *self.polling_active.borrow_mut() = false;
        if let Some(id) = self.update_source_id.borrow_mut().take() {
            id.remove();
        }
        self.stop_scan_timer();
    }

    fn start_scan_timer(self: &Rc<Self>) {
        if self.scan_source_id.borrow().is_some() || !*self.networks_visible.borrow() {
            return;
        }
        let weak_self = Rc::downgrade(self);
        let id = glib::timeout_add_local(SCAN_INTERVAL, move || {
            if let Some(inner_self) = weak_self.upgrade() {
                if !inner_self.popover.is_visible() || !*inner_self.networks_visible.borrow() {
                    *inner_self.scan_source_id.borrow_mut() = None;
                    return glib::ControlFlow::Break;
                }
                inner_self.trigger_scan();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        *self.scan_source_id.borrow_mut() = Some(id);
    }

    pub fn stop_scan_timer(self: &Rc<Self>) {
        if let Some(id) = self.scan_source_id.borrow_mut().take() {
            id.remove();
        }
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }

    pub fn networks_revealer(&self) -> Option<Revealer> {
        self.ui_elements
            .borrow()
            .as_ref()
            .map(|ui| ui.networks_revealer.clone())
    }

    pub fn available_networks_button_icon(&self) -> Option<Image> {
        self.ui_elements
            .borrow()
            .as_ref()
            .map(|ui| ui.available_networks_button_icon.clone())
    }
}

impl Drop for NetworkWindow {
    fn drop(&mut self) {
        if let Some(id) = self.update_source_id.borrow_mut().take() {
            id.remove();
        }
        if let Some(id) = self.scan_source_id.borrow_mut().take() {
            id.remove();
        }
    }
}
