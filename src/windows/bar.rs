use crate::utils::{apply_css, get_distro_icon_name, BarConfig};
use crate::windows::AppMenu;
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box, MenuButton, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};

pub struct BarWindow {
    window: ApplicationWindow,
}

impl BarWindow {
    pub fn new(app: &gtk4::Application, config: &BarConfig) -> Self {
        let window = ApplicationWindow::builder().application(app).build();

        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.auto_exclusive_zone_enable();
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);

        let container = Box::new(Orientation::Horizontal, 6);
        container.set_margin_start(6);
        container.set_margin_end(6);
        container.set_margin_top(2);
        container.set_margin_bottom(2);
        container.add_css_class("container");

        let menu_button = MenuButton::new();
        menu_button.add_css_class("app-menu-button"); // Add this line

        let icon_name = config.distro_icon_override.clone().or_else(|| {
            match get_distro_icon_name() {
                Ok(Some(name)) => Some(name),
                Ok(None) => {
                    eprintln!("Could not determine specific distribution icon from /etc/os-release. Trying 'distributor-logo'.");
                    Some("distributor-logo".to_string())
                }
                Err(e) => {
                    eprintln!("Failed to get distribution icon: {}. Trying 'distributor-logo'.", e);
                    Some("distributor-logo".to_string())
                }
            }
        }).unwrap_or_else(|| "open-menu-symbolic".to_string());

        menu_button.set_icon_name(&icon_name);

        let menu = AppMenu::new();
        menu_button.set_popover(Some(menu.popover()));

        container.append(&menu_button);

        window.set_default_size(-1, config.height);

        apply_css();

        window.set_child(Some(&container));

        Self { window }
    }

    pub fn present(&self) {
        self.window.present();
    }
}
