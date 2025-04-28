use crate::utils::{
    battery::BatteryService,
    config::{BarConfig, BatteryConfig, ModuleType, NetworkConfig},
    get_distro_icon_name,
    network::NetworkCommand,
};
use crate::widgets::{ActiveClientWidget, BatteryWidget, NetworkWidget};
use crate::windows::{AppMenu, BatteryWindow, DateWindow, NetworkWindow};
use chrono::Local;
use glib::source::timeout_add_local;
use glib::ControlFlow;
use gtk4::prelude::*;
use gtk4::{
    glib, Application, ApplicationWindow, Box as GtkBox, Button, Label, MenuButton, Orientation,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::{cell::RefCell, rc::Rc, time::Duration};
use tokio::sync::mpsc;

pub struct BarWindow {
    pub window: ApplicationWindow,
    _date_popover_provider: DateWindow,
    _app_menu: Option<Rc<AppMenu>>,
    _battery_window: Option<Rc<BatteryWindow>>,
    pub network_widget: Option<Rc<NetworkWidget>>,
    pub network_window: Option<Rc<NetworkWindow>>,
}

impl BarWindow {
    pub fn new(
        app: &Application,
        config: &BarConfig,
        net_command_tx: mpsc::Sender<NetworkCommand>,
        network_service_available: bool,
    ) -> Self {
        let battery_service = Rc::new(RefCell::new(
            BatteryService::new().expect("Failed to initialize shared BatteryService"),
        ));

        let window = ApplicationWindow::builder().application(app).build();
        window.add_css_class("Bar");
        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.auto_exclusive_zone_enable();
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_keyboard_mode(KeyboardMode::None);

        let container = GtkBox::new(Orientation::Horizontal, 0);
        let left_box = GtkBox::new(Orientation::Horizontal, 6);
        left_box.set_halign(gtk4::Align::Start);
        left_box.add_css_class("left-box");
        let center_box = GtkBox::new(Orientation::Horizontal, 6);
        center_box.set_halign(gtk4::Align::Center);
        center_box.set_hexpand(true);
        center_box.add_css_class("center-box");
        let right_box = GtkBox::new(Orientation::Horizontal, 6);
        right_box.set_halign(gtk4::Align::End);
        right_box.add_css_class("right-box");

        let mut app_menu_instance: Option<Rc<AppMenu>> = None;
        let mut battery_window_instance: Option<Rc<BatteryWindow>> = None;
        let mut network_widget_instance: Option<Rc<NetworkWidget>> = None;
        let mut network_window_instance: Option<Rc<NetworkWindow>> = None;

        let fmt = config
            .clock_format
            .clone()
            .unwrap_or_else(|| "%A %e, %H:%M".to_string());

        let date_window_instance = DateWindow::new(config);
        let date_popover = date_window_instance.popover().clone();

        let battery_service_clone = battery_service.clone();
        let config_clone_app_menu = config.clone();
        let window_weak = window.downgrade();

        let mut add_module = |m: &ModuleType,
                              target: &GtkBox,
                              cfg: &BarConfig,
                              battery_cfg: &BatteryConfig,
                              network_cfg: &NetworkConfig| {
            let battery_service_for_module = battery_service_clone.clone();
            let net_command_tx_clone = net_command_tx.clone();

            match m {
                ModuleType::AppMenu => {
                    let btn = MenuButton::new();
                    btn.add_css_class("app-menu-button");
                    let icon = config_clone_app_menu
                        .distro_icon_override
                        .clone()
                        .or_else(|| {
                            get_distro_icon_name()
                                .ok()
                                .flatten()
                                .or(Some("distributor-logo".to_string()))
                        })
                        .unwrap_or_else(|| "open-menu-symbolic".to_string());
                    btn.set_icon_name(&icon);

                    let menu = AppMenu::new();
                    btn.set_popover(Some(menu.popover()));

                    let popover = menu.popover().clone();
                    let window_weak_clone_show = window_weak.clone();
                    popover.connect_show(move |_| {
                        if let Some(window) = window_weak_clone_show.upgrade() {
                            window.set_keyboard_mode(KeyboardMode::Exclusive);
                        }
                    });

                    let window_weak_clone_closed = window_weak.clone();
                    popover.connect_closed(move |_| {
                        if let Some(window) = window_weak_clone_closed.upgrade() {
                            window.set_keyboard_mode(KeyboardMode::None);
                        }
                    });

                    app_menu_instance = Some(menu);
                    target.append(&btn);
                }
                ModuleType::ActiveClient => {
                    let w = ActiveClientWidget::new(cfg.active_client_max_length);
                    target.append(w.widget());
                }
                ModuleType::Clock => {
                    let clock_button = Button::new();
                    clock_button.add_css_class("clock-button");

                    let lbl = Label::new(None);
                    let now = Local::now();
                    lbl.set_text(&now.format(&fmt).to_string());
                    clock_button.set_child(Some(&lbl));

                    let lbl_clone = lbl.clone();
                    let fmt_clone = fmt.clone();
                    let update_interval = if fmt_clone.contains("%S") {
                        Duration::from_secs(1)
                    } else {
                        Duration::from_secs(30)
                    };

                    timeout_add_local(update_interval, move || {
                        let now = Local::now();
                        let time_str = now.format(&fmt_clone).to_string();
                        lbl_clone.set_label(&time_str);
                        ControlFlow::Continue
                    });

                    let popover_clone = date_popover.clone();
                    popover_clone.set_parent(&clock_button);

                    clock_button.connect_clicked(move |button| {
                        popover_clone.set_pointing_to(Some(&button.allocation()));
                        popover_clone.popup();
                    });

                    target.append(&clock_button);
                }
                ModuleType::Battery => {
                    let battery_widget = BatteryWidget::new(battery_service_for_module.clone());
                    let battery_button = battery_widget.widget();

                    let bw_instance = BatteryWindow::new(battery_cfg);
                    let battery_popover = bw_instance.popover().clone();
                    battery_popover.set_parent(battery_button);

                    battery_button.connect_clicked(move |button| {
                        battery_popover.set_pointing_to(Some(&button.allocation()));
                        battery_popover.popup();
                    });

                    battery_window_instance = Some(bw_instance);
                    target.append(battery_button);
                }
                ModuleType::Network => {
                    if network_service_available {
                        let network_widget = NetworkWidget::new(net_command_tx_clone.clone());
                        let network_button = network_widget.widget().clone();

                        let nw_instance = NetworkWindow::new(network_cfg, net_command_tx_clone);
                        let network_popover = nw_instance.popover().clone();
                        let network_popover_clone = network_popover.clone();
                        network_popover.set_parent(&network_button);

                        let revealer_opt = nw_instance.networks_revealer();
                        let icon_opt = nw_instance.available_networks_button_icon();
                        let nw_instance_clone = nw_instance.clone();

                        network_button.connect_clicked(move |button| {
                            network_popover.set_pointing_to(Some(&button.allocation()));
                            network_popover.popup();
                        });

                        network_popover_clone.connect_closed(move |_| {
                            if let Some(revealer) = &revealer_opt {
                                if revealer.reveals_child() {
                                    revealer.set_reveal_child(false);
                                    if let Some(icon) = &icon_opt {
                                        icon.set_icon_name(Some("pan-down-symbolic"));
                                        icon.remove_css_class("expanded");
                                    }
                                    *nw_instance_clone.networks_visible.borrow_mut() = false;
                                    nw_instance_clone.stop_scan_timer();
                                }
                            }
                        });

                        network_widget_instance = Some(network_widget);
                        network_window_instance = Some(nw_instance);
                        target.append(&network_button);
                    } else {
                        eprintln!("Network module enabled in config, but NetworkService failed to initialize.");
                        let error_label = Label::new(Some("Network N/A"));
                        target.append(&error_label);
                    }
                }
            }
        };

        for m in &config.modules_left {
            add_module(m, &left_box, config, &config.battery, &config.network);
        }
        for m in &config.modules_center {
            add_module(m, &center_box, config, &config.battery, &config.network);
        }
        for m in &config.modules_right {
            add_module(m, &right_box, config, &config.battery, &config.network);
        }

        container.append(&left_box);
        container.append(&center_box);
        container.append(&right_box);
        window.set_child(Some(&container));

        BarWindow {
            window,
            _date_popover_provider: date_window_instance,
            _app_menu: app_menu_instance,
            _battery_window: battery_window_instance,
            network_widget: network_widget_instance,
            network_window: network_window_instance,
        }
    }

    pub fn present(&self) {
        self.window.present();
    }
}
