use crate::utils::display_control::{
    get_brightness, get_color_temperature,
    is_night_light_on, kelvin_to_slider,
    set_brightness, set_night_light, slider_to_kelvin,
    DEFAULT_TEMP, MAX_TEMP, MIN_TEMP,
};
use std::time::Duration;
use std::sync::atomic::{AtomicU32, Ordering};
use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    gio, glib,
    Button, Label, Orientation, Popover, Revealer, Scale,
};
use gtk4::{Box as GtkBox};

const REFRESH_INTERVAL_WINDOW: u32 = 2000;
const TEMP_SLIDER_DEBOUNCE_MS: u64 = 500;

const GSETTINGS_INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";
const GSETTINGS_COLOR_SCHEME_KEY: &str = "color-scheme";
const COLOR_SCHEME_PREFER_DARK: &str = "prefer-dark";
const COLOR_SCHEME_DEFAULT: &str = "default";

struct DisplayControlWindowUI {
    brightness_slider: Scale,
    brightness_label: Label,
    night_light_button: Button,
    dark_mode_button: Button,
    temp_revealer: Revealer,
    temp_slider: Scale,
    temp_label: Label,
}

#[derive(Clone)]
pub struct DisplayControlWindow {
    popover: Popover,
    ui_elements: Rc<RefCell<Option<DisplayControlWindowUI>>>,
    brightness_value: Rc<RefCell<f64>>,
    night_light_enabled: Rc<RefCell<bool>>,
    color_temp_value: Rc<RefCell<u32>>,
    dark_mode_enabled: Rc<RefCell<bool>>,
    polling_active: Rc<RefCell<bool>>,
    update_source_id: Rc<RefCell<Option<glib::SourceId>>>,
    brightness_update_lock: Rc<RefCell<bool>>,
    temp_update_lock: Rc<RefCell<bool>>,
    slider_action_active: Rc<RefCell<bool>>,
    ui_update_lock: Rc<RefCell<bool>>,
    gsettings: Option<gio::Settings>,
    gsettings_changed_handler_id: Rc<RefCell<Option<glib::SignalHandlerId>>>,
    temp_slider_debounce_id: Rc<RefCell<Option<glib::SourceId>>>,
    last_temp_value: Rc<AtomicU32>,
}

impl DisplayControlWindow {
    pub fn new() -> Rc<Self> {
        let popover = Popover::new();
        popover.add_css_class("DisplayControlWindow");
        popover.set_autohide(true);

        let ui_elements = Rc::new(RefCell::new(None));
        let brightness_value = Rc::new(RefCell::new(0.0));
        let night_light_enabled = Rc::new(RefCell::new(false));
        let color_temp_value = Rc::new(RefCell::new(DEFAULT_TEMP));
        let dark_mode_enabled = Rc::new(RefCell::new(false));
        let polling_active = Rc::new(RefCell::new(false));
        let update_source_id = Rc::new(RefCell::new(None));
        let brightness_update_lock = Rc::new(RefCell::new(false));
        let temp_update_lock = Rc::new(RefCell::new(false));
        let slider_action_active = Rc::new(RefCell::new(false));
        let ui_update_lock = Rc::new(RefCell::new(false));

        let gsettings = gio::SettingsSchemaSource::default()
            .and_then(|source| source.lookup(GSETTINGS_INTERFACE_SCHEMA, true))
            .map(|schema| gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None));

        let gsettings_changed_handler_id = Rc::new(RefCell::new(None));
        let temp_slider_debounce_id = Rc::new(RefCell::new(None));
        let last_temp_value = Rc::new(AtomicU32::new(DEFAULT_TEMP));

        let window = Rc::new(Self {
            popover,
            ui_elements,
            brightness_value,
            night_light_enabled,
            color_temp_value,
            dark_mode_enabled,
            polling_active,
            update_source_id,
            brightness_update_lock,
            temp_update_lock,
            slider_action_active,
            ui_update_lock,
            gsettings,
            gsettings_changed_handler_id,
            temp_slider_debounce_id,
            last_temp_value,
        });

        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        let (brightness_section, brightness_slider, brightness_label) =
            Self::build_brightness_control(&window);

        let (toggles_section, night_light_button, dark_mode_button, temp_revealer, temp_slider, temp_label) =
            Self::build_quick_toggles(&window);

        let settings_section = Self::build_settings_section(&window);

        container.append(&brightness_section);
        container.append(&toggles_section);
        container.append(&settings_section);

        window.popover.set_child(Some(&container));

        window.ui_elements.replace(Some(DisplayControlWindowUI {
            brightness_slider,
            brightness_label,
            night_light_button,
            dark_mode_button,
            temp_revealer,
            temp_slider,
            temp_label,
        }));

        let weak_w = Rc::downgrade(&window);
        window.popover.connect_hide(move |_| {
            if let Some(w) = weak_w.upgrade() {
                *w.polling_active.borrow_mut() = false;
            }
        });

        let weak_w = Rc::downgrade(&window);
        window.popover.connect_show(move |_| {
            if let Some(w) = weak_w.upgrade() {
                w.start_polling();
            }
        });

        let window_clone = window.clone();
        glib::idle_add_local_once(move || {
            let handle = tokio::runtime::Handle::current();
            let _ = handle.block_on(async {
                match is_night_light_on().await {
                    Ok(night_light) => {
                        window_clone.update_night_light_state(Some(night_light));
                        
                        if night_light {
                            if let Ok(temp) = get_color_temperature().await {
                                window_clone.last_temp_value.store(temp, Ordering::SeqCst);
                                if let Some(ui) = window_clone.ui_elements.borrow().as_ref() {
                                    ui.temp_slider.set_value(kelvin_to_slider(temp));
                                    ui.temp_label.set_label(&format!("{}K", temp));
                                }
                            }
                        }
                    },
                    Err(e) => eprintln!("Failed to initialize night light state: {}", e),
                }
            });
        });

        window.start_polling();

        if let Some(settings) = &window.gsettings {
            let window_dm = window.clone();
            let id = settings.connect_changed(
                Some(GSETTINGS_COLOR_SCHEME_KEY),
                move |settings, _key| {
                    window_dm.update_dark_mode_state_from_settings(settings);
                },
            );
            *window.gsettings_changed_handler_id.borrow_mut() = Some(id);
            window.update_dark_mode_state_from_settings(settings);
        }

        window
    }

    fn build_brightness_control(window: &Rc<Self>) -> (GtkBox, Scale, Label) {
        let brightness_label = Label::builder()
            .label("N/A")
            .xalign(1.0)
            .css_classes(vec!["setting-value"])
            .build();

        let brightness_slider = Scale::builder()
            .orientation(Orientation::Horizontal)
            .draw_value(false)
            .hexpand(true)
            .build();
        brightness_slider.set_range(0.0, 1.0);
        brightness_slider.add_css_class("brightness-slider");

        let weak_window_brightness = Rc::downgrade(window);
        let update_lock_clone = window.brightness_update_lock.clone();

        brightness_slider.connect_change_value(move |_slider, _, value| {
            if *update_lock_clone.borrow() {
                return glib::Propagation::Stop;
            }

            if let Some(window) = weak_window_brightness.upgrade() {
                *window.brightness_value.borrow_mut() = value;

                if let Some(ui) = window.ui_elements.borrow().as_ref() {
                    ui.brightness_label.set_label(&format!("{}%", (value * 100.0).round() as i32));
                }

                let _ = glib::idle_add_local_once(move || {
                    let _ = set_brightness(value);
                });
            }

            glib::Propagation::Stop
        });

        let section_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .build();
        section_box.append(&brightness_slider);
        section_box.append(&brightness_label);

        (section_box, brightness_slider, brightness_label)
    }

    fn build_quick_toggle_button(icon_name: &str, tooltip: &str, class: &str) -> Button {
        let button = Button::builder()
            .icon_name(icon_name)
            .tooltip_text(tooltip)
            .hexpand(true)
            .vexpand(true)
            .css_classes(vec!["toggle-button", class])
            .build();
        button
    }

    fn build_quick_toggles(window: &Rc<Self>) -> (GtkBox, Button, Button, Revealer, Scale, Label) {
        let night_light_button =
            Self::build_quick_toggle_button("night-light-symbolic", "Night Light", "night-light");
        let dark_mode_button =
            Self::build_quick_toggle_button("dark-mode-symbolic", "Dark Mode", "dark-mode");

        let toggles_row = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(vec!["toggles-row"])
            .build();
        toggles_row.append(&night_light_button);
        toggles_row.append(&dark_mode_button);

        let temp_label = Label::builder()
            .label("N/A")
            .xalign(1.0)
            .css_classes(vec!["setting-value"])
            .build();

        let temp_slider = Scale::builder()
            .orientation(Orientation::Horizontal)
            .draw_value(false)
            .hexpand(true)
            .build();
        temp_slider.set_range(0.0, 1.0);

        let temp_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .build();
        temp_box.append(&temp_slider);
        temp_box.append(&temp_label);

        let temp_revealer = Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .child(&temp_box)
            .reveal_child(false)
            .build();

        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(12)
            .css_classes(vec!["quick-toggles"])
            .build();
        container.append(&toggles_row);
        container.append(&temp_revealer);

        let window_nl = Rc::downgrade(window);
        night_light_button.connect_clicked(move |_| {
            if let Some(window) = window_nl.upgrade() {
                let current_state = *window.night_light_enabled.borrow();
                let new_state = !current_state;
                let window_clone = window.clone();

                let temp = if new_state {
                    window.last_temp_value.load(Ordering::SeqCst)
                        .max(MIN_TEMP)
                        .min(MAX_TEMP)
                } else {
                    DEFAULT_TEMP
                };

                if let Some(ui) = window.ui_elements.borrow().as_ref() {
                    if new_state {
                        ui.night_light_button.add_css_class("active");
                        ui.temp_revealer.set_reveal_child(true);
                        ui.temp_slider.set_sensitive(true);

                        if let Ok(mut temp_lock) = window.temp_update_lock.try_borrow_mut() {
                            *temp_lock = true;
                            ui.temp_slider.set_value(kelvin_to_slider(temp));
                            ui.temp_label.set_label(&format!("{}K", temp));
                            *temp_lock = false;
                        }
                    } else {
                        ui.night_light_button.remove_css_class("active");
                        ui.temp_revealer.set_reveal_child(false);
                        ui.temp_slider.set_sensitive(false);
                    }
                }

                let _ = glib::idle_add_local_once(move || {
                    let handle = tokio::runtime::Handle::current();
                    let _ = handle.block_on(async {
                        match set_night_light(new_state, temp).await {
                            Ok(_) => window_clone.update_night_light_state(Some(new_state)),
                            Err(e) => {
                                eprintln!("Failed to set night light: {}", e);
                                window_clone.update_night_light_state(Some(false));
                            },
                        }
                    });
                });
            }
        });

        let window_temp = Rc::downgrade(window);
        let debounce_id_clone = window.temp_slider_debounce_id.clone();

        temp_slider.connect_value_changed(move |slider| {
            if let Some(window) = window_temp.upgrade() {
                if !*window.night_light_enabled.borrow() {
                    return;
                }

                let slider_value = slider.value();
                let kelvin = slider_to_kelvin(slider_value);

                if let Some(ui) = window.ui_elements.borrow().as_ref() {
                    ui.temp_label.set_label(&format!("{}K", kelvin));
                }

                window.last_temp_value.store(kelvin, Ordering::SeqCst);

                if let Some(id) = debounce_id_clone.borrow_mut().take() {
                    id.remove();
                }

                let window_timeout = window.clone();

                let source_id = glib::timeout_add_local(
                    Duration::from_millis(TEMP_SLIDER_DEBOUNCE_MS),
                    move || {
                        let win = &window_timeout;
                        if !*win.night_light_enabled.borrow() {
                            return glib::ControlFlow::Break;
                        }

                        let kelvin_to_apply = win.last_temp_value.load(Ordering::SeqCst)
                            .clamp(MIN_TEMP, MAX_TEMP);

                        let window_timeout_clone = window_timeout.clone();
                        let _ = glib::idle_add_local_once(move || {
                            let handle = tokio::runtime::Handle::current();
                            let _ = handle.block_on(async {
                                match set_night_light(true, kelvin_to_apply).await {
                                    Ok(_) => {
                                    },
                                    Err(e) => {
                                        eprintln!("Failed to set night light temperature: {}", e);
                                        let window_weak = Rc::downgrade(&window_timeout_clone);
                                        let _ = glib::idle_add_local_once(move || {
                                            if let Some(w) = window_weak.upgrade() {
                                                w.update_night_light_state(Some(false));
                                            }
                                        });
                                    },
                                }
                            });
                        });

                        glib::ControlFlow::Break
                    }
                );

                *debounce_id_clone.borrow_mut() = Some(source_id);
            }
        });

        let window_dm = Rc::downgrade(window);
        dark_mode_button.connect_clicked(move |_| {
            if let Some(window) = window_dm.upgrade() {
                if let Some(settings) = &window.gsettings {
                    let current_state = *window.dark_mode_enabled.borrow();
                    let new_scheme = if current_state {
                        COLOR_SCHEME_DEFAULT
                    } else {
                        COLOR_SCHEME_PREFER_DARK
                    };

                    let _ = settings.set_string(GSETTINGS_COLOR_SCHEME_KEY, new_scheme);
                }
            }
        });

        (container, night_light_button, dark_mode_button, temp_revealer, temp_slider, temp_label)
    }

    fn build_settings_section(window: &Rc<Self>) -> GtkBox {
        let section_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .build();

        let button = Button::builder()
            .label("Settings")
            .hexpand(true)
            .css_classes(vec!["settings-button"])
            .build();

        let weak_window = Rc::downgrade(window);
        button.connect_clicked(move |_| {
            if let Some(w) = weak_window.upgrade() {
                w.popover.popdown();
                let _ = glib::spawn_command_line_async("gnome-control-center display");
            }
        });

        section_box.append(&button);
        section_box
    }

    fn update_night_light_state(&self, state: Option<bool>) {
        let enabled = state.unwrap_or(false);
        *self.night_light_enabled.borrow_mut() = enabled;

        if let Some(ui) = self.ui_elements.borrow().as_ref() {
            if enabled {
                ui.night_light_button.add_css_class("active");
                ui.temp_revealer.set_reveal_child(true);
                ui.temp_slider.set_sensitive(true);

                let temp = self.last_temp_value.load(Ordering::SeqCst)
                    .max(MIN_TEMP)
                    .min(MAX_TEMP);

                ui.temp_slider.set_value(kelvin_to_slider(temp));
                ui.temp_label.set_label(&format!("{}K", temp));
            } else {
                ui.night_light_button.remove_css_class("active");
                ui.temp_revealer.set_reveal_child(false);
                ui.temp_slider.set_sensitive(false);
            }

            ui.night_light_button.set_sensitive(true);
        }
    }

    fn update_dark_mode_state(&self, state: Option<bool>) {
        *self.dark_mode_enabled.borrow_mut() = state.unwrap_or(false);

        if let Some(ui) = self.ui_elements.borrow().as_ref() {
            if let Some(enabled) = state {
                if enabled {
                    ui.dark_mode_button.add_css_class("active");
                } else {
                    ui.dark_mode_button.remove_css_class("active");
                }
            }
        }
    }

    fn update_dark_mode_state_from_settings(&self, settings: &gio::Settings) {
        let scheme = settings.string(GSETTINGS_COLOR_SCHEME_KEY);
        self.update_dark_mode_state(Some(scheme.as_str() == COLOR_SCHEME_PREFER_DARK));
    }

    fn start_polling(self: &Rc<Self>) {
        if *self.polling_active.borrow() {
            return;
        }
        *self.polling_active.borrow_mut() = true;
        let weak_self = Rc::downgrade(self);

        let source_id = glib::timeout_add_local(
            Duration::from_millis(REFRESH_INTERVAL_WINDOW as u64),
            move || {
                if let Some(window) = weak_self.upgrade() {
                    if !*window.polling_active.borrow() || !window.popover.is_visible() {
                        return glib::ControlFlow::Break;
                    }


                    let window_weak = Rc::downgrade(&window);

                    let _ = glib::idle_add_local_once(move || {
                        if let Some(window) = window_weak.upgrade() {
                            let window_clone_brightness = window.clone();
                            let _ = glib::idle_add_local_once(move || {
                                let handle = tokio::runtime::Handle::current();
                                let _ = handle.block_on(async {
                                    match get_brightness().await {
                                        Ok(brightness) => {
                                            let window_weak = Rc::downgrade(&window_clone_brightness);
                                            let _ = glib::idle_add_local_once(move || {
                                                if let Some(window) = window_weak.upgrade() {
                                                    if !*window.slider_action_active.borrow() {
                                                        *window.brightness_update_lock.borrow_mut() = true;
                                                        *window.brightness_value.borrow_mut() = brightness;
                                                        if let Some(ui) = window.ui_elements.borrow().as_ref() {
                                                            ui.brightness_slider.set_value(brightness);
                                                            ui.brightness_label.set_label(&format!("{}%", (brightness * 100.0).round() as i32));
                                                        }
                                                        *window.brightness_update_lock.borrow_mut() = false;
                                                    }
                                                }
                                            });
                                        },
                                        Err(e) => eprintln!("Failed to get brightness: {}", e),
                                    }
                                });
                            });

                            let window_clone_night_light = window.clone();
                            let _ = glib::idle_add_local_once(move || {
                                let handle = tokio::runtime::Handle::current();
                                let _ = handle.block_on(async {
                                    match is_night_light_on().await {
                                        Ok(night_light) => {
                                            let window_weak = Rc::downgrade(&window_clone_night_light);
                                            let _ = glib::idle_add_local_once(move || {
                                                if let Some(window) = window_weak.upgrade() {
                                                    window.update_night_light_state(Some(night_light));

                                                    if night_light {
                                                        let window_clone_temp = window.clone();
                                                        let _ = glib::idle_add_local_once(move || {
                                                            let handle = tokio::runtime::Handle::current();
                                                            let _ = handle.block_on(async {
                                                                match get_color_temperature().await {
                                                                    Ok(temp) => {
                                                                        let window_weak = Rc::downgrade(&window_clone_temp);
                                                                        let _ = glib::idle_add_local_once(move || {
                                                                            if let Some(window) = window_weak.upgrade() {
                                                                                window.last_temp_value.store(temp, Ordering::SeqCst);
                                                                                if !*window.temp_update_lock.borrow() {
                                                                                    if let Some(ui) = window.ui_elements.borrow().as_ref() {
                                                                                        ui.temp_slider.set_value(kelvin_to_slider(temp));
                                                                                        ui.temp_label.set_label(&format!("{}K", temp));
                                                                                    }
                                                                                }
                                                                            }
                                                                        });
                                                                    },
                                                                    Err(e) => eprintln!("Failed to get color temperature: {}", e),
                                                                }
                                                            });
                                                        });
                                                    }
                                                }
                                            });
                                        },
                                        Err(e) => {
                                            eprintln!("Failed to check night light status: {}", e);
                                        },
                                    }
                                });
                            });
                        }
                    });

                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            },
        );

        *self.update_source_id.borrow_mut() = Some(source_id);
    }

    fn stop_polling(self: &Rc<Self>) {
        if let Some(_source_id) = self.update_source_id.borrow_mut().take() {
        }
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }
    
    pub fn refresh_ui_state(&self) {
        let window_clone = Rc::new(self.clone());
        glib::idle_add_local_once(move || {
            let handle = tokio::runtime::Handle::current();
            let _ = handle.block_on(async {
                if let Ok(brightness) = get_brightness().await {
                    window_clone.brightness_value.replace(brightness);
                    if let Some(ui) = window_clone.ui_elements.borrow().as_ref() {
                        ui.brightness_slider.set_value(brightness);
                        ui.brightness_label.set_label(&format!("{}%", (brightness * 100.0).round() as i32));
                    }
                }
                
                if let Ok(night_light) = is_night_light_on().await {
                    window_clone.update_night_light_state(Some(night_light));
                    
                    if night_light {
                        if let Ok(temp) = get_color_temperature().await {
                            window_clone.last_temp_value.store(temp, Ordering::SeqCst);
                            if let Some(ui) = window_clone.ui_elements.borrow().as_ref() {
                                ui.temp_slider.set_value(kelvin_to_slider(temp));
                                ui.temp_label.set_label(&format!("{}K", temp));
                            }
                        }
                    }
                }
            });
        });
    }
}



impl Drop for DisplayControlWindow {
    fn drop(&mut self) {
        *self.polling_active.borrow_mut() = false;
    }
}
