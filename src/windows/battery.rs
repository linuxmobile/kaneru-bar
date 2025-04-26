use crate::utils::{
    battery::{
        format_charge_status, get_active_power_profile, get_available_power_profiles,
        get_conservation_mode, set_conservation_mode, set_power_profile, BatteryDetails,
        BatteryService, BatteryUtilError, PowerProfile,
    },
    BarConfig,
};
use battery::State;
use gtk4::prelude::*;
use gtk4::{glib, Align, Box as GtkBox, Button, Image, Label, Orientation, Popover};
use std::{cell::RefCell, rc::Rc, time::Duration};

const REFRESH_INTERVAL_WINDOW: Duration = Duration::from_secs(5);

struct BatteryWindowUI {
    main_icon: Image,
    percentage_label: Label,
    status_label: Label,
    health_label: Label,
    cycles_label: Label,
    power_draw_label: Label,
    voltage_label: Label,
    power_profile_buttons_box: GtkBox,
    conservation_mode_button: Button,
    conservation_mode_status_icon: Image,
}

pub struct BatteryWindow {
    popover: Popover,
    service: Rc<RefCell<BatteryService>>,
    details: Rc<RefCell<Option<BatteryDetails>>>,
    active_profile: Rc<RefCell<Option<PowerProfile>>>,
    conservation_mode: Rc<RefCell<Option<bool>>>,
    ui_elements: Rc<RefCell<Option<BatteryWindowUI>>>,
    _update_source_id: Rc<RefCell<Option<glib::SourceId>>>,
}

impl BatteryWindow {
    pub fn new(_config: &BarConfig) -> Rc<Self> {
        let popover = Popover::new();
        popover.add_css_class("BatteryWindow");
        popover.set_autohide(true);

        let service = Rc::new(RefCell::new(
            BatteryService::new().expect("Failed to initialize BatteryService"),
        ));
        let details = Rc::new(RefCell::new(None));
        let active_profile = Rc::new(RefCell::new(None));
        let conservation_mode = Rc::new(RefCell::new(None));
        let ui_elements = Rc::new(RefCell::new(None));
        let update_source_id = Rc::new(RefCell::new(None));

        let window = Rc::new(Self {
            popover: popover.clone(),
            service,
            details: details.clone(),
            active_profile: active_profile.clone(),
            conservation_mode: conservation_mode.clone(),
            ui_elements: ui_elements.clone(),
            _update_source_id: update_source_id.clone(),
        });

        let main_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .width_request(350)
            .build();

        let (main_info_box, main_icon, percentage_label, status_label) =
            Self::build_main_info_section();
        main_box.append(&main_info_box);

        let (details_container, health_label, cycles_label, power_draw_label, voltage_label) =
            Self::build_details_section();
        main_box.append(&details_container);

        let (power_profile_section, power_profile_buttons_box) =
            Self::build_power_profile_section();
        main_box.append(&power_profile_section);

        let (conservation_section, conservation_mode_button, conservation_mode_status_icon) =
            Self::build_conservation_mode_section(window.clone());
        main_box.append(&conservation_section);

        let settings_section = Self::build_settings_section(popover.clone());
        main_box.append(&settings_section);

        popover.set_child(Some(&main_box));

        *ui_elements.borrow_mut() = Some(BatteryWindowUI {
            main_icon,
            percentage_label,
            status_label,
            health_label,
            cycles_label,
            power_draw_label,
            voltage_label,
            power_profile_buttons_box,
            conservation_mode_button,
            conservation_mode_status_icon,
        });

        let window_clone = window.clone();
        popover.connect_visible_notify(move |pop| {
            if pop.is_visible() {
                window_clone.update_all_data();
                window_clone.start_polling();
            } else {
                window_clone.stop_polling();
            }
        });

        window
    }

    fn build_main_info_section() -> (GtkBox, Image, Label, Label) {
        let main_icon = Image::builder()
            .icon_name("battery-missing-symbolic")
            .css_classes(vec!["icon-large"])
            .build();

        let percentage_label = Label::builder()
            .label("Battery N/A")
            .halign(Align::Start)
            .css_classes(vec!["title-1"])
            .build();

        let status_label = Label::builder()
            .label("State N/A")
            .halign(Align::Start)
            .css_classes(vec!["caption"])
            .build();

        let text_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .hexpand(true)
            .build();
        text_box.append(&percentage_label);
        text_box.append(&status_label);

        let main_info_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .css_classes(vec!["battery-main-info"])
            .build();
        main_info_box.append(&main_icon);
        main_info_box.append(&text_box);

        (main_info_box, main_icon, percentage_label, status_label)
    }

    fn build_details_section() -> (GtkBox, Label, Label, Label, Label) {
        let (health_row, health_label) = Self::create_detail_row("Health:");
        let (cycles_row, cycles_label) = Self::create_detail_row("Charge cycles:");
        let (power_draw_row, power_draw_label) = Self::create_detail_row("Power draw:");
        let (voltage_row, voltage_label) = Self::create_detail_row("Voltage:");

        let details_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(5)
            .css_classes(vec!["battery-details"])
            .build();
        details_box.append(&health_row);
        details_box.append(&cycles_row);
        details_box.append(&power_draw_row);
        details_box.append(&voltage_row);

        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(10)
            .css_classes(vec!["battery-info-container"])
            .build();
        container.append(&details_box);

        (
            container,
            health_label,
            cycles_label,
            power_draw_label,
            voltage_label,
        )
    }

    fn create_detail_row(label_text: &str) -> (GtkBox, Label) {
        let label = Label::builder().label(label_text).build();
        let value_label = Label::builder()
            .label("N/A")
            .halign(Align::End)
            .hexpand(true)
            .build();
        let row_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .hexpand(true)
            .build();
        row_box.append(&label);
        row_box.append(&value_label);
        (row_box, value_label)
    }

    fn build_power_profile_section() -> (GtkBox, GtkBox) {
        let buttons_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .css_classes(vec!["power-mode-buttons"])
            .hexpand(true)
            .halign(Align::Fill)
            .build();

        let section_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(10)
            .css_classes(vec!["power-profiles-section"])
            .hexpand(true)
            .build();

        let title = Label::builder()
            .label("Power Mode")
            .halign(Align::Start)
            .css_classes(vec!["title-2"])
            .build();

        section_box.append(&title);
        section_box.append(&buttons_box);

        (section_box, buttons_box)
    }

    fn build_conservation_mode_section(window_rc: Rc<Self>) -> (GtkBox, Button, Image) {
        let status_icon = Image::builder()
            .icon_name("emblem-important-symbolic")
            .build();

        let button = Button::builder()
            .css_classes(vec!["conservation-mode-button"])
            .hexpand(true)
            .build();

        let icon = Image::builder().icon_name("battery-good-symbolic").build();

        let label_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .build();
        label_box.append(
            &Label::builder()
                .label("Battery Conservation Mode")
                .halign(Align::Start)
                .build(),
        );
        label_box.append(
            &Label::builder()
                .label("Limit charge to 80% to extend lifespan")
                .halign(Align::Start)
                .css_classes(vec!["caption", "dim-label"])
                .build(),
        );

        let button_content = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .hexpand(true)
            .build();
        button_content.append(&icon);
        button_content.append(&label_box);
        button_content.append(&status_icon);
        button.set_child(Some(&button_content));

        let window_clone = window_rc.clone();
        let status_icon_clone = status_icon.clone();
        button.connect_clicked(move |btn| {
            let current_state = window_clone.conservation_mode.borrow().unwrap_or(false);
            let new_state = !current_state;
            match set_conservation_mode(new_state) {
                Ok(_) => {
                    window_clone.update_conservation_mode_data();
                    Self::update_conservation_button_style(btn, new_state, &status_icon_clone);
                    window_clone.update_ui();
                }
                Err(e) => {
                    eprintln!("Failed to set conservation mode: {}", e);
                }
            }
        });

        let section_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(10)
            .css_classes(vec!["conservation-mode-section"])
            .hexpand(true)
            .build();

        let title = Label::builder()
            .label("Battery Settings")
            .halign(Align::Start)
            .css_classes(vec!["title-2"])
            .build();

        section_box.append(&title);
        section_box.append(&button);

        (section_box, button, status_icon)
    }

    fn build_settings_section(popover: Popover) -> GtkBox {
        let button = Button::builder()
            .label("Power & battery settings")
            .css_classes(vec!["settings-button"])
            .hexpand(true)
            .build();

        button.connect_clicked(move |_| {
            popover.popdown();
            let _ = std::process::Command::new("env")
                .args(["XDG_CURRENT_DESKTOP=GNOME", "gnome-control-center", "power"])
                .spawn();
        });

        let section_box = GtkBox::builder()
            .css_classes(vec!["settings-section"])
            .hexpand(true)
            .build();
        section_box.append(&button);
        section_box
    }

    fn update_all_data(self: &Rc<Self>) {
        self.update_battery_details();
        self.update_power_profile_data();
        self.update_conservation_mode_data();
        self.update_ui();
    }

    fn update_battery_details(self: &Rc<Self>) {
        match self.service.borrow_mut().get_primary_battery_details() {
            Ok(d) => *self.details.borrow_mut() = Some(d),
            Err(e) => {
                eprintln!("Failed to get battery details: {}", e);
                *self.details.borrow_mut() = None;
            }
        }
    }

    fn update_power_profile_data(self: &Rc<Self>) {
        match get_active_power_profile() {
            Ok(p) => *self.active_profile.borrow_mut() = Some(p),
            Err(e) => {
                eprintln!("Failed to get active power profile: {}", e);
                *self.active_profile.borrow_mut() = None;
            }
        }
    }

    fn update_conservation_mode_data(self: &Rc<Self>) {
        match get_conservation_mode() {
            Ok(b) => *self.conservation_mode.borrow_mut() = Some(b),
            Err(BatteryUtilError::SysfsNotFound(_))
            | Err(BatteryUtilError::PermissionDenied(_)) => {
                *self.conservation_mode.borrow_mut() = None;
                if let Some(ui) = self.ui_elements.borrow().as_ref() {
                    if let Some(parent) = ui.conservation_mode_button.parent() {
                        if let Some(section) = parent.parent() {
                            section.set_visible(false);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to get conservation mode: {}", e);
                *self.conservation_mode.borrow_mut() = Some(false);
            }
        }
    }

    fn update_ui(self: &Rc<Self>) {
        let ui_opt = self.ui_elements.borrow();
        let ui = match ui_opt.as_ref() {
            Some(ui) => ui,
            None => return,
        };
        let details_opt = self.details.borrow();
        let details = details_opt.as_ref();
        let conservation_mode_enabled = self.conservation_mode.borrow().unwrap_or(false);

        if let Some(d) = details {
            ui.main_icon.set_icon_name(Some(
                d.icon_name.as_deref().unwrap_or("battery-missing-symbolic"),
            ));
            ui.percentage_label
                .set_label(&format!("Battery {:.0}%", d.percentage.unwrap_or(0.0)));

            let mut status_text = format_charge_status(d);
            if conservation_mode_enabled
                && (d.state == Some(State::Full) || d.state == Some(State::Unknown))
            {
                status_text = "Conservation Mode".to_string();
            }
            ui.status_label.set_label(&status_text);

            ui.health_label
                .set_label(&format!("{:.0}%", d.health_percentage.unwrap_or(0.0)));
            ui.cycles_label
                .set_label(&d.cycle_count.map_or("N/A".to_string(), |c| c.to_string()));
            ui.power_draw_label
                .set_label(&format!("{:.1} W", d.energy_rate_watts.unwrap_or(0.0)));
            ui.voltage_label
                .set_label(&format!("{:.1} V", d.voltage_volts.unwrap_or(0.0)));
        } else {
            ui.main_icon.set_icon_name(Some("battery-missing-symbolic"));
            ui.percentage_label.set_label("Battery N/A");
            ui.status_label.set_label("State N/A");
            ui.health_label.set_label("N/A");
            ui.cycles_label.set_label("N/A");
            ui.power_draw_label.set_label("N/A");
            ui.voltage_label.set_label("N/A");
        }

        let active_profile_opt = self.active_profile.borrow();
        let active_p = active_profile_opt.as_ref();

        Self::rebuild_power_profile_buttons(self, &ui.power_profile_buttons_box, active_p);

        if let Some(enabled) = self.conservation_mode.borrow().as_ref() {
            if let Some(parent) = ui.conservation_mode_button.parent() {
                if let Some(section) = parent.parent() {
                    section.set_visible(true);
                }
            }
            Self::update_conservation_button_style(
                &ui.conservation_mode_button,
                *enabled,
                &ui.conservation_mode_status_icon,
            );
        } else {
            if let Some(parent) = ui.conservation_mode_button.parent() {
                if let Some(section) = parent.parent() {
                    section.set_visible(false);
                }
            }
        }
    }

    fn rebuild_power_profile_buttons(
        window_rc: &Rc<Self>,
        buttons_box: &GtkBox,
        active_profile: Option<&PowerProfile>,
    ) {
        while let Some(child) = buttons_box.first_child() {
            buttons_box.remove(&child);
        }

        match get_available_power_profiles() {
            Ok(available_profiles) => {
                if available_profiles.is_empty() {
                    if let Some(parent) = buttons_box.parent() {
                        parent.set_visible(false);
                    }
                    return;
                } else {
                    if let Some(parent) = buttons_box.parent() {
                        parent.set_visible(true);
                    }
                }

                for profile in available_profiles {
                    let label_str = match profile {
                        PowerProfile::PowerSaver => "Power Saver",
                        PowerProfile::Balanced => "Balanced",
                        PowerProfile::Performance => "Performance",
                        PowerProfile::Unknown(ref s) => s.as_str(),
                    };
                    let button = Button::builder()
                        .label(label_str)
                        .css_classes(vec!["power-mode-button"])
                        .hexpand(true)
                        .build();

                    if Some(&profile) == active_profile {
                        button.add_css_class("active");
                    }

                    let profile_clone = profile.clone();
                    let weak_window = Rc::downgrade(window_rc);
                    button.connect_clicked(move |_| {
                        if let Some(window) = weak_window.upgrade() {
                            match set_power_profile(profile_clone.clone()) {
                                Ok(_) => {
                                    window.update_power_profile_data();
                                    window.update_ui();
                                }
                                Err(e) => {
                                    eprintln!("Failed to set power profile: {}", e);
                                }
                            }
                        }
                    });
                    buttons_box.append(&button);
                }
            }
            Err(e) => {
                eprintln!("Failed to get available power profiles: {}", e);
                if let Some(parent) = buttons_box.parent() {
                    parent.set_visible(false);
                }
            }
        }
    }

    fn update_conservation_button_style(button: &Button, enabled: bool, status_icon: &Image) {
        if enabled {
            button.add_css_class("active");
            status_icon.set_icon_name(Some("emblem-ok-symbolic"));
        } else {
            button.remove_css_class("active");
            status_icon.set_icon_name(Some("emblem-important-symbolic"));
        }
    }

    fn start_polling(self: &Rc<Self>) {
        if self._update_source_id.borrow().is_some() {
            return;
        }

        let weak_self = Rc::downgrade(self);
        let id = glib::timeout_add_local(REFRESH_INTERVAL_WINDOW, move || {
            if let Some(inner_self) = weak_self.upgrade() {
                inner_self.update_all_data();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        *self._update_source_id.borrow_mut() = Some(id);
    }

    fn stop_polling(self: &Rc<Self>) {
        if let Some(id) = self._update_source_id.borrow_mut().take() {
            id.remove();
        }
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }
}

impl Drop for BatteryWindow {
    fn drop(&mut self) {}
}
