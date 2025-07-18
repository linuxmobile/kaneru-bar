use crate::utils::{config::DockConfig, AppResolver};
use crate::utils::niri;
use gtk4::prelude::*;
use gtk4::{glib, Application, ApplicationWindow, Box as GtkBox, Button, EventControllerMotion, Image, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::{cell::RefCell, rc::Rc, time::Duration};

pub struct DockWindow {
    trigger_window: ApplicationWindow,
    dock_window: Rc<RefCell<Option<ApplicationWindow>>>,
    app: Application,
    config: DockConfig,
    hide_timer: Rc<RefCell<Option<glib::SourceId>>>,
    trigger_hover: Rc<RefCell<bool>>,
    dock_hover: Rc<RefCell<bool>>,
}



impl DockWindow {
    pub fn new(app: &Application, config: &DockConfig) -> Rc<Self> {
        let trigger_window = ApplicationWindow::builder()
            .application(app)
            .build();

        trigger_window.init_layer_shell();
        trigger_window.set_layer(Layer::Top);
        trigger_window.set_namespace(Some("kaneru-dock-trigger"));
        trigger_window.set_keyboard_mode(KeyboardMode::None);

        Self::setup_trigger_positioning(&trigger_window, config);

        let trigger_area = GtkBox::new(Orientation::Horizontal, 0);
        trigger_area.set_height_request(5);
        trigger_area.set_hexpand(true);
        trigger_area.add_css_class("dock-trigger");

        trigger_window.set_child(Some(&trigger_area));

        let dock = Rc::new(Self {
            trigger_window: trigger_window.clone(),
            dock_window: Rc::new(RefCell::new(None)),
            app: app.clone(),
            config: config.clone(),
            hide_timer: Rc::new(RefCell::new(None)),
            trigger_hover: Rc::new(RefCell::new(false)),
            dock_hover: Rc::new(RefCell::new(false)),
        });

        if config.auto_hide {
            dock.setup_auto_hide(&trigger_area);
            trigger_window.present();
        } else {
            dock.create_dock_window();
        }

        dock
    }

    fn setup_trigger_positioning(window: &ApplicationWindow, config: &DockConfig) {
        match config.position {
            crate::utils::config::DockPosition::Bottom => {
                window.set_anchor(Edge::Bottom, true);
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Right, true);
                window.set_anchor(Edge::Top, false);
                window.set_margin(Edge::Bottom, 0);
            }
            crate::utils::config::DockPosition::Left => {
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Top, true);
                window.set_anchor(Edge::Bottom, true);
                window.set_anchor(Edge::Right, false);
                window.set_margin(Edge::Left, 0);
            }
            crate::utils::config::DockPosition::Right => {
                window.set_anchor(Edge::Right, true);
                window.set_anchor(Edge::Top, true);
                window.set_anchor(Edge::Bottom, true);
                window.set_anchor(Edge::Left, false);
                window.set_margin(Edge::Right, 0);
            }
        }
    }

    fn setup_dock_positioning(window: &ApplicationWindow, config: &DockConfig) {
        match config.position {
            crate::utils::config::DockPosition::Bottom => {
                window.set_anchor(Edge::Bottom, true);
                window.set_anchor(Edge::Left, false);
                window.set_anchor(Edge::Right, false);
                window.set_anchor(Edge::Top, false);
                window.set_margin(Edge::Bottom, 0);
            }
            crate::utils::config::DockPosition::Left => {
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Top, false);
                window.set_anchor(Edge::Bottom, false);
                window.set_anchor(Edge::Right, false);
                window.set_margin(Edge::Left, 0);
            }
            crate::utils::config::DockPosition::Right => {
                window.set_anchor(Edge::Right, true);
                window.set_anchor(Edge::Top, false);
                window.set_anchor(Edge::Bottom, false);
                window.set_anchor(Edge::Left, false);
                window.set_margin(Edge::Right, 0);
            }
        }
    }

    fn setup_auto_hide(self: &Rc<Self>, trigger_area: &GtkBox) {
        let trigger_hover = self.trigger_hover.clone();
        let dock_hover = self.dock_hover.clone();
        let dock_weak = Rc::downgrade(self);

        let trigger_hover_enter = trigger_hover.clone();
        let dock_weak_enter = dock_weak.clone();
        let motion_controller = EventControllerMotion::new();
        motion_controller.connect_enter(move |_, _, _| {
            *trigger_hover_enter.borrow_mut() = true;
            if let Some(dock) = dock_weak_enter.upgrade() {
                dock.reveal();
            }
        });

        let trigger_hover_leave = trigger_hover.clone();
        let dock_hover_leave = dock_hover.clone();
        let dock_weak_leave = dock_weak.clone();
        motion_controller.connect_leave(move |_| {
            *trigger_hover_leave.borrow_mut() = false;
            let dock_hover = *dock_hover_leave.borrow();
            if !dock_hover {
                if let Some(dock) = dock_weak_leave.upgrade() {
                    dock.schedule_hide_with_hover();
                }
            }
        });

        trigger_area.add_controller(motion_controller);
    }

    fn create_dock_window(self: &Rc<Self>) {
        let window = ApplicationWindow::builder()
            .application(&self.app)
            .build();

        window.add_css_class("Dock");
        window.add_css_class("revealed");
        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.set_namespace(Some("kaneru-dock"));
        window.set_keyboard_mode(KeyboardMode::None);

        Self::setup_dock_positioning(&window, &self.config);

        let dock_wrapper = GtkBox::new(Orientation::Vertical, 0);
        dock_wrapper.add_css_class("dock-wrapper");
        dock_wrapper.set_halign(gtk4::Align::Center);
        dock_wrapper.set_valign(gtk4::Align::End);

        let container = GtkBox::new(Orientation::Horizontal, 4);
        container.add_css_class("dock-container");
        container.set_halign(gtk4::Align::Center);

        let dock_hover = self.dock_hover.clone();
        let trigger_hover = self.trigger_hover.clone();

        let app_resolver = AppResolver::new();

        let windows = niri::get_windows().unwrap_or_default();
        let focused = niri::get_focused_window().ok().flatten();

        let mut open_desktop_ids = vec![];
        let mut open_execs = vec![];
        for w in &windows {
            if let Some(app_id) = &w.app_id {
                open_desktop_ids.push(app_id.to_lowercase());
            }
            if let Some(title) = &w.title {
                open_execs.push(title.to_lowercase());
            }
        }
        let focused_app_id = focused.as_ref().and_then(|w| w.app_id.clone()).map(|s| s.to_lowercase());

        for app_name in &self.config.favorites {
            if let Some(app_info) = app_resolver.resolve(app_name) {
                let command = app_info.exec.clone();

                let icon_name = if app_info.icon.contains("/") || app_info.icon.is_empty() {
                    "application-x-executable"
                } else {
                    &app_info.icon
                };

                let icon = Image::builder()
                    .icon_name(icon_name)
                    .pixel_size(40)
                    .build();

                let tooltip = if app_info.name.len() > 50 {
                    app_name
                } else {
                    &app_info.name
                };

                let button_box = GtkBox::new(Orientation::Vertical, 0);
                button_box.set_valign(gtk4::Align::Fill);
                button_box.set_halign(gtk4::Align::Center);
                button_box.append(&icon);

                let desktop_id = app_info.desktop_id.to_lowercase();
                let exec_base = app_resolver.extract_command_name(&app_info.exec).to_lowercase();

                let is_open = open_desktop_ids.iter().any(|id| id == &desktop_id)
                    || open_execs.iter().any(|ex| ex == &exec_base);
                let is_active = focused_app_id.as_ref().map(|id| id == &desktop_id || id == &exec_base).unwrap_or(false);

                if is_open || is_active {
                    let indicator = GtkBox::new(Orientation::Horizontal, 0);
                    indicator.add_css_class("indicator");
                    indicator.set_valign(gtk4::Align::End);
                    indicator.set_halign(gtk4::Align::Center);
                    button_box.append(&indicator);
                }

                let button = Button::builder()
                    .child(&button_box)
                    .tooltip_text(tooltip)
                    .build();
                button.set_can_focus(false);
                button.add_css_class("dock-icon");

                if is_open {
                    button.add_css_class("open");
                }
                if is_active {
                    button.add_css_class("active");
                }

                let parts = crate::utils::app_resolver::AppResolver::clean_exec(&command);
                button.connect_clicked(move |_| {
                    if !parts.is_empty() {
                        let mut cmd = std::process::Command::new(&parts[0]);
                        if parts.len() > 1 {
                            cmd.args(&parts[1..]);
                        }
                        if let Err(e) = cmd.spawn() {
                            eprintln!("Failed to launch {:?}: {}", parts, e);
                        }
                    }
                });
                container.append(&button);
            }
        }

        for w in &windows {
            let app_id = match &w.app_id {
                Some(id) => id.to_lowercase(),
                None => continue,
            };
            if self.config.favorites.iter().any(|fav| {
                if let Some(info) = app_resolver.resolve(fav) {
                    let desktop_id = info.desktop_id.to_lowercase();
                    let exec_base = app_resolver.extract_command_name(&info.exec).to_lowercase();
                    app_id == desktop_id || app_id == exec_base
                } else {
                    false
                }
            }) {
                continue;
            }

            let (icon_name, command) = if let Some(app_info) = app_resolver.resolve_by_desktop_id(&app_id) {
                (
                    if app_info.icon.contains("/") || app_info.icon.is_empty() {
                        "application-x-executable"
                    } else {
                        &app_info.icon
                    }.to_string(),
                    app_info.exec.clone(),
                )
            } else {
                ("application-x-executable".to_string(), app_id.clone())
            };

            let icon = Image::builder()
                .icon_name(&icon_name)
                .pixel_size(40)
                .build();

            let button_box = GtkBox::new(Orientation::Vertical, 0);
            button_box.set_valign(gtk4::Align::Fill);
            button_box.set_halign(gtk4::Align::Center);
            button_box.append(&icon);

            let is_active = focused_app_id.as_ref().map(|id| id == &app_id).unwrap_or(false);

            if is_active || true {
                let indicator = GtkBox::new(Orientation::Horizontal, 0);
                indicator.add_css_class("indicator");
                indicator.set_valign(gtk4::Align::End);
                indicator.set_halign(gtk4::Align::Center);
                button_box.append(&indicator);
            }

            let button = Button::builder()
                .child(&button_box)
                .tooltip_text(&app_id)
                .build();
            button.set_can_focus(false);

            let parts = crate::utils::app_resolver::AppResolver::clean_exec(&command);
            button.connect_clicked(move |_| {
                if !parts.is_empty() {
                    let mut cmd = std::process::Command::new(&parts[0]);
                    if parts.len() > 1 {
                        cmd.args(&parts[1..]);
                    }
                    if let Err(e) = cmd.spawn() {
                        eprintln!("Failed to launch {:?}: {}", parts, e);
                    }
                }
            });

            button.add_css_class("dock-icon");
            button.add_css_class("open");
            if is_active {
                button.add_css_class("active");
            }
            container.append(&button);
        }

        dock_wrapper.append(&container);
        window.set_child(Some(&dock_wrapper));

        let dock_hover_enter = dock_hover.clone();
        let dock_weak_enter = Rc::downgrade(self);
        let motion_controller = EventControllerMotion::new();
        motion_controller.connect_enter(move |_, _, _| {
            *dock_hover_enter.borrow_mut() = true;
            if let Some(dock) = dock_weak_enter.upgrade() {
                if let Some(timer_id) = dock.hide_timer.borrow_mut().take() {
                    timer_id.remove();
                }
            }
        });

        let dock_hover_leave = dock_hover.clone();
        let trigger_hover_leave = trigger_hover.clone();
        let dock_weak_leave = Rc::downgrade(self);
        motion_controller.connect_leave(move |_| {
            *dock_hover_leave.borrow_mut() = false;
            let trigger_hover_val = *trigger_hover_leave.borrow();
            if !trigger_hover_val {
                if let Some(dock) = dock_weak_leave.upgrade() {
                    dock.schedule_hide_with_hover();
                }
            }
        });

        window.add_controller(motion_controller);

        window.present();
        *self.dock_window.borrow_mut() = Some(window);
    }

    fn reveal(self: &Rc<Self>) {
        if let Some(timer_id) = self.hide_timer.borrow_mut().take() {
            timer_id.remove();
        }

        if self.dock_window.borrow().is_none() {
            self.create_dock_window();
        }
    }

    fn schedule_hide_with_hover(self: &Rc<Self>) {
        if self.dock_window.borrow().is_some() && self.hide_timer.borrow().is_none() {
            let dock_weak = Rc::downgrade(self);
            let trigger_hover = self.trigger_hover.clone();
            let dock_hover = self.dock_hover.clone();
            let hide_delay = self.config.hide_delay;
            let timer_id = glib::timeout_add_local_once(
                Duration::from_millis(hide_delay as u64),
                move || {
                    let trigger = *trigger_hover.borrow();
                    let dock = *dock_hover.borrow();
                    if !trigger && !dock {
                        if let Some(dock) = dock_weak.upgrade() {
                            dock.hide();
                            *dock.hide_timer.borrow_mut() = None;
                        }
                    }
                },
            );
            *self.hide_timer.borrow_mut() = Some(timer_id);
        }
    }

    fn hide(self: &Rc<Self>) {
        if let Some(window) = self.dock_window.borrow_mut().take() {
            window.close();
        }
    }



    pub fn present(&self) {
        if let Some(dock_window) = self.dock_window.borrow().as_ref() {
            dock_window.present();
        } else {
            self.trigger_window.present();
        }
    }
}
