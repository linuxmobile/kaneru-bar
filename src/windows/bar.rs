use crate::utils::{get_distro_icon_name, BarConfig, ModuleType};
use crate::widgets::ActiveClientWidget;
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

        let center_box = Box::new(Orientation::Horizontal, 6);
        center_box.set_halign(gtk4::Align::Center);
        center_box.set_hexpand(true);
        center_box.add_css_class("center-box");

        let right_box = Box::new(Orientation::Horizontal, 6);
        right_box.set_halign(gtk4::Align::End);
        right_box.add_css_class("right-box");

        let mut app_menu_instance_temp: Option<AppMenu> = None;
        let mut app_menu_button_temp: Option<MenuButton> = None;

        let create_and_add_widget =
            |module_type: &ModuleType,
             target_box: &Box,
             config: &BarConfig,
             app_menu_instance_temp: &mut Option<AppMenu>,
             app_menu_button_temp: &mut Option<MenuButton>| {
                match module_type {
                    ModuleType::AppMenu => {
                        let menu_button = MenuButton::new();
                        menu_button.add_css_class("app-menu-button");

                        let icon_name = config
                            .distro_icon_override
                            .clone()
                            .or_else(|| {
                                get_distro_icon_name()
                                    .ok()
                                    .flatten()
                                    .or(Some("distributor-logo".to_string()))
                            })
                            .unwrap_or_else(|| "open-menu-symbolic".to_string());
                        menu_button.set_icon_name(&icon_name);

                        let app_menu = AppMenu::new();
                        *app_menu_instance_temp = Some(app_menu);
                        *app_menu_button_temp = Some(menu_button.clone());

                        target_box.append(&menu_button);
                        println!("Added AppMenu module");
                    }
                    ModuleType::ActiveClient => {
                        let widget = ActiveClientWidget::new();
                        target_box.append(widget.widget());
                        println!("Added ActiveClient module");
                    }
                }
            };

        for module_type in &config.modules_left {
            create_and_add_widget(
                module_type,
                &left_box,
                config,
                &mut app_menu_instance_temp,
                &mut app_menu_button_temp,
            );
        }

        for module_type in &config.modules_center {
            create_and_add_widget(
                module_type,
                &center_box,
                config,
                &mut app_menu_instance_temp,
                &mut app_menu_button_temp,
            );
        }

        for module_type in &config.modules_right {
            create_and_add_widget(
                module_type,
                &right_box,
                config,
                &mut app_menu_instance_temp,
                &mut app_menu_button_temp,
            );
        }

        if let (Some(button), Some(menu)) = (&app_menu_button_temp, &app_menu_instance_temp) {
            button.set_popover(Some(menu.popover()));
            println!("Connected AppMenu popover to its button.");
        } else if app_menu_button_temp.is_some() != app_menu_instance_temp.is_some() {
            eprintln!("Warning: AppMenu button and instance mismatch during creation.");
        }

        container.append(&left_box);
        container.append(&center_box);
        container.append(&right_box);

        window.set_child(Some(&container));

        Self { window }
    }

    pub fn present(&self) {
        self.window.present();
    }
}
