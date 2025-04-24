use crate::utils::{apply_css, get_distro_icon_name, BarConfig};
use crate::widgets::ActiveClientWidget;
use crate::windows::AppMenu;
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box, MenuButton, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};

pub struct BarWindow {
    window: ApplicationWindow,
    _active_client_widget: ActiveClientWidget,
    _app_menu: AppMenu,
}

impl BarWindow {
    pub fn new(app: &gtk4::Application, config: &BarConfig) -> Self {
        let window = ApplicationWindow::builder().application(app).build();

        window.add_css_class("Bar");

        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.auto_exclusive_zone_enable();
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);

        let container = Box::new(Orientation::Horizontal, 0);

        let left_box = Box::new(Orientation::Horizontal, 6);
        left_box.set_halign(gtk4::Align::Start);
        left_box.add_css_class("left-box");
        container.append(&left_box);

        let menu_button = MenuButton::new();
        menu_button.add_css_class("app-menu-button");

        let icon_name = config
            .distro_icon_override
            .clone()
            .or_else(|| {
                match get_distro_icon_name() {
                    Ok(Some(name)) => Some(name),
                    Ok(None) => {
                        eprintln!("Could not determine specific distribution icon from /etc/os-release. Trying 'distributor-logo'.");
                        Some("distributor-logo".to_string())
                    }
                    Err(e) => {
                        eprintln!(
                            "Failed to get distribution icon: {}. Trying 'distributor-logo'.",
                            e
                        );
                        Some("distributor-logo".to_string())
                    }
                }
            })
            .unwrap_or_else(|| "open-menu-symbolic".to_string());

        menu_button.set_icon_name(&icon_name);

        let app_menu = AppMenu::new();
        menu_button.set_popover(Some(app_menu.popover()));

        left_box.append(&menu_button);

        let active_client_widget = ActiveClientWidget::new();
        left_box.append(active_client_widget.widget());

        let center_box = Box::new(Orientation::Horizontal, 6);
        center_box.set_halign(gtk4::Align::Center);
        center_box.set_hexpand(true);
        center_box.add_css_class("center-box");
        container.append(&center_box);

        let right_box = Box::new(Orientation::Horizontal, 6);
        right_box.set_halign(gtk4::Align::End);
        right_box.add_css_class("right-box");
        container.append(&right_box);

        window.set_default_size(-1, config.height);

        apply_css();

        window.set_child(Some(&container));

        Self {
            window,
            _active_client_widget: active_client_widget,
            _app_menu: app_menu,
        }
    }

    pub fn present(&self) {
        self.window.present();
    }
}
