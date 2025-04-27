use crate::utils::{
    battery::{
        format_charge_status, get_active_power_profile, get_available_power_profiles,
        get_conservation_mode, set_conservation_mode, set_power_profile, BatteryDetails,
        BatteryService, BatteryUtilError, PowerProfile,
    },
    config::BatteryConfig,
};
use battery::State;
use gtk4::prelude::*;
use gtk4::{glib, Align, Box as GtkBox, Button, Image, Label, Orientation, Popover};
use std::{cell::RefCell, rc::Rc, time::Duration};
use tokio::task;

const REFRESH_INTERVAL_WINDOW: Duration = Duration::from_secs(5);

struct BatteryWindowUI {
    details_container: GtkBox,
    power_profile_section: GtkBox,
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
    config: BatteryConfig,
    service: Rc<RefCell<BatteryService>>,
    details: Rc<RefCell<Option<BatteryDetails>>>,
    active_profile: Rc<RefCell<Option<PowerProfile>>>,
    available_profiles: Rc<RefCell<Option<Result<Vec<PowerProfile>, BatteryUtilError>>>>,
    conservation_mode: Rc<RefCell<Option<bool>>>,
    ui_elements: Rc<RefCell<Option<BatteryWindowUI>>>,
    polling_active: Rc<RefCell<bool>>,
    update_source_id: Rc<RefCell<Option<glib::SourceId>>>,
}

impl BatteryWindow {
    pub fn new(config: &BatteryConfig) -> Rc<Self> {
        let popover = Popover::new();
        popover.add_css_class("BatteryWindow");
        popover.set_autohide(true);

        let service = Rc::new(RefCell::new(
            BatteryService::new().expect("Failed to initialize BatteryService"),
        ));
        let details = Rc::new(RefCell::new(None));
        let active_profile = Rc::new(RefCell::new(None));
        let available_profiles = Rc::new(RefCell::new(None));
        let conservation_mode = Rc::new(RefCell::new(None));
        let ui_elements = Rc::new(RefCell::new(None));
        let update_source_id = Rc::new(RefCell::new(None));
        let polling_active = Rc::new(RefCell::new(false));

        let window = Rc::new(Self {
            popover: popover.clone(),
            config: config.clone(),
            service,
            details: details.clone(),
            active_profile: active_profile.clone(),
            available_profiles: available_profiles.clone(),
            conservation_mode: conservation_mode.clone(),
            ui_elements: ui_elements.clone(),
            polling_active: polling_active.clone(),
            update_source_id: update_source_id.clone(),
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
        details_container.set_visible(config.show_details);
        main_box.append(&details_container);

        let (power_profile_section, power_profile_buttons_box) =
            Self::build_power_profile_section();
        power_profile_section.set_visible(config.show_power_profiles);
        main_box.append(&power_profile_section);

        let (conservation_section, conservation_mode_button, conservation_mode_status_icon) =
            Self::build_conservation_mode_section(window.clone());

        let conservation_should_be_visible =
            config.show_conservation_mode && config.conservation_mode_path.is_some();
        conservation_section.set_visible(conservation_should_be_visible);
        main_box.append(&conservation_section);

        let settings_section = Self::build_settings_section(popover.clone());
        main_box.append(&settings_section);

        popover.set_child(Some(&main_box));

        *ui_elements.borrow_mut() = Some(BatteryWindowUI {
            details_container,
            power_profile_section,
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

        let weak_window = Rc::downgrade(&window_rc);
        let status_icon_clone = status_icon.clone();
        button.connect_clicked(move |btn| {
            if let Some(window) = weak_window.upgrade() {
                let conservation_path = window.config.conservation_mode_path.clone();
                let Some(path) = conservation_path else {
                    eprintln!("Conservation mode path not configured.");
                    return;
                };

                let current_state = window.conservation_mode.borrow().unwrap_or(false);
                let new_state = !current_state;
                let btn_clone = btn.clone();
                let status_icon_clone_async = status_icon_clone.clone();
                let weak_window_async = weak_window.clone();

                glib::MainContext::default().spawn_local(async move {
                    let result =
                        task::spawn_blocking(move || set_conservation_mode(new_state, &path)).await;
                    if let Some(window) = weak_window_async.upgrade() {
                        match result {
                            Ok(Ok(_)) => {
                                *window.conservation_mode.borrow_mut() = Some(new_state);
                                Self::update_conservation_button_style(
                                    &btn_clone,
                                    new_state,
                                    &status_icon_clone_async,
                                );
                                btn_clone.set_sensitive(true);
                                window.update_ui();
                            }
                            Ok(Err(e)) => {
                                eprintln!("Failed to set conservation mode: {}", e);
                                *window.conservation_mode.borrow_mut() = None;
                                btn_clone.set_sensitive(false);
                                Self::update_conservation_button_style(
                                    &btn_clone,
                                    false,
                                    &status_icon_clone_async,
                                );
                            }
                            Err(join_err) => {
                                eprintln!(
                                    "Tokio task failed for set_conservation_mode: {}",
                                    join_err
                                );
                                *window.conservation_mode.borrow_mut() = None;
                                btn_clone.set_sensitive(false);
                                Self::update_conservation_button_style(
                                    &btn_clone,
                                    false,
                                    &status_icon_clone_async,
                                );
                            }
                        }
                    }
                });
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

    async fn update_all_data(self: &Rc<Self>) {
        self.update_battery_details();

        let active_profile_res = task::spawn_blocking(get_active_power_profile).await;
        let available_profiles_res = task::spawn_blocking(get_available_power_profiles).await;

        let conservation_path_opt = self.config.conservation_mode_path.clone();
        let conservation_res = if let Some(path) = conservation_path_opt {
            task::spawn_blocking(move || get_conservation_mode(&path)).await
        } else {
            Ok(Ok(false))
        };

        match active_profile_res {
            Ok(Ok(p)) => *self.active_profile.borrow_mut() = Some(p),
            Ok(Err(e)) => {
                eprintln!("Failed to get active power profile: {}", e);
                *self.active_profile.borrow_mut() = None;
            }
            Err(e) => {
                eprintln!("Tokio task failed for get_active_power_profile: {}", e);
                *self.active_profile.borrow_mut() = None;
            }
        }

        match available_profiles_res {
            Ok(Ok(p_vec)) => *self.available_profiles.borrow_mut() = Some(Ok(p_vec)),
            Ok(Err(e)) => {
                eprintln!("Failed to get available power profiles: {}", e);
                *self.available_profiles.borrow_mut() = Some(Err(e));
            }
            Err(e) => {
                eprintln!("Tokio task failed for get_available_power_profiles: {}", e);
                *self.available_profiles.borrow_mut() = None;
            }
        }

        if self.config.conservation_mode_path.is_some() {
            match conservation_res {
                Ok(Ok(b)) => {
                    *self.conservation_mode.borrow_mut() = Some(b);
                }
                Ok(Err(e)) => {
                    eprintln!("Failed to get conservation mode state: {}", e);
                    *self.conservation_mode.borrow_mut() = None;
                }
                Err(e) => {
                    eprintln!("Tokio task failed for get_conservation_mode: {}", e);
                    *self.conservation_mode.borrow_mut() = None;
                }
            }
        } else {
            *self.conservation_mode.borrow_mut() = None;
        }

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

    fn update_ui(self: &Rc<Self>) {
        let ui_opt = self.ui_elements.borrow();
        let ui = match ui_opt.as_ref() {
            Some(ui) => ui,
            None => return,
        };
        let details_opt = self.details.borrow();
        let details = details_opt.as_ref();
        let conservation_mode_state = self.conservation_mode.borrow();

        if let Some(d) = details {
            ui.main_icon.set_icon_name(Some(
                d.icon_name.as_deref().unwrap_or("battery-missing-symbolic"),
            ));
            ui.percentage_label
                .set_label(&format!("Battery {:.0}%", d.percentage.unwrap_or(0.0)));

            let mut status_text = format_charge_status(d);
            if conservation_mode_state.unwrap_or(false)
                && (d.state == Some(State::Full) || d.state == Some(State::Unknown))
            {
                status_text = "Conservation Mode".to_string();
            }
            ui.status_label.set_label(&status_text);

            ui.details_container.set_visible(self.config.show_details);
            if self.config.show_details {
                ui.health_label
                    .set_label(&format!("{:.0}%", d.health_percentage.unwrap_or(0.0)));
                ui.cycles_label
                    .set_label(&d.cycle_count.map_or("N/A".to_string(), |c| c.to_string()));
                ui.power_draw_label
                    .set_label(&format!("{:.1} W", d.energy_rate_watts.unwrap_or(0.0)));
                ui.voltage_label
                    .set_label(&format!("{:.1} V", d.voltage_volts.unwrap_or(0.0)));
            }
        } else {
            ui.main_icon.set_icon_name(Some("battery-missing-symbolic"));
            ui.percentage_label.set_label("Battery N/A");
            ui.status_label.set_label("State N/A");
            ui.details_container.set_visible(self.config.show_details);
            if self.config.show_details {
                ui.health_label.set_label("N/A");
                ui.cycles_label.set_label("N/A");
                ui.power_draw_label.set_label("N/A");
                ui.voltage_label.set_label("N/A");
            }
        }

        ui.power_profile_section
            .set_visible(self.config.show_power_profiles);
        if self.config.show_power_profiles {
            let active_profile_opt = self.active_profile.borrow();
            let available_profiles_opt = self.available_profiles.borrow();
            self.rebuild_power_profile_buttons(
                &ui.power_profile_buttons_box,
                active_profile_opt.as_ref(),
                available_profiles_opt.as_ref(),
            );
        }

        let conservation_should_be_visible =
            self.config.show_conservation_mode && self.config.conservation_mode_path.is_some();

        if let Some(parent) = ui.conservation_mode_button.parent() {
            if let Some(section) = parent.parent().and_then(|p| p.downcast::<GtkBox>().ok()) {
                section.set_visible(conservation_should_be_visible);
            }
        }

        if conservation_should_be_visible {
            match *conservation_mode_state {
                Some(enabled) => {
                    ui.conservation_mode_button.set_sensitive(true);
                    Self::update_conservation_button_style(
                        &ui.conservation_mode_button,
                        enabled,
                        &ui.conservation_mode_status_icon,
                    );
                }
                None => {
                    ui.conservation_mode_button.set_sensitive(false);
                    Self::update_conservation_button_style(
                        &ui.conservation_mode_button,
                        false,
                        &ui.conservation_mode_status_icon,
                    );
                }
            }
        }
    }

    fn rebuild_power_profile_buttons(
        self: &Rc<Self>,
        buttons_box: &GtkBox,
        active_profile: Option<&PowerProfile>,
        available_profiles_res: Option<&Result<Vec<PowerProfile>, BatteryUtilError>>,
    ) {
        while let Some(child) = buttons_box.first_child() {
            buttons_box.remove(&child);
        }

        match available_profiles_res {
            Some(Ok(available_profiles)) => {
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

                    if Some(profile) == active_profile {
                        button.add_css_class("active");
                    }

                    let profile_clone = profile.clone();
                    let weak_window = Rc::downgrade(self);
                    button.connect_clicked(move |_| {
                        if let Some(_window) = weak_window.upgrade() {
                            let profile_to_set = profile_clone.clone();
                            let weak_window_async = weak_window.clone();
                            glib::MainContext::default().spawn_local(async move {
                                let result =
                                    task::spawn_blocking(move || set_power_profile(profile_to_set))
                                        .await;
                                if let Some(window) = weak_window_async.upgrade() {
                                    match result {
                                        Ok(Ok(_)) => {
                                            window.update_all_data().await;
                                        }
                                        Ok(Err(e)) => {
                                            eprintln!("Failed to set power profile: {}", e);
                                        }
                                        Err(join_err) => {
                                            eprintln!(
                                                "Tokio task failed for set_power_profile: {}",
                                                join_err
                                            );
                                        }
                                    }
                                }
                            });
                        }
                    });
                    buttons_box.append(&button);
                }
            }
            Some(Err(e)) => {
                eprintln!("Error getting available power profiles: {}", e);
                if let Some(parent) = buttons_box.parent() {
                    parent.set_visible(false);
                }
            }
            None => {
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
    }

    fn stop_polling(self: &Rc<Self>) {
        *self.polling_active.borrow_mut() = false;
        if let Some(id) = self.update_source_id.borrow_mut().take() {
            id.remove();
        }
    }

    pub fn popover(&self) -> &Popover {
        &self.popover
    }
}

impl Drop for BatteryWindow {
    fn drop(&mut self) {
        if let Some(id) = self.update_source_id.borrow_mut().take() {
            id.remove();
        }
    }
}
