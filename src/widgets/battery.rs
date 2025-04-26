use crate::utils::battery::{
    format_charge_status, BatteryDetails, BatteryService, BatteryUtilError,
};
use gtk4::prelude::*;
use gtk4::{glib, Align, Box as GtkBox, Button, Image, Label, Orientation};
use std::{cell::RefCell, rc::Rc, time::Duration};

const REFRESH_INTERVAL: Duration = Duration::from_secs(15);

struct BatteryState {
    service: Option<BatteryService>,
}

pub struct BatteryWidget {
    container: Button,
    icon: Image,
    label: Label,
    state: Rc<RefCell<BatteryState>>,
    _update_source_id: Option<glib::SourceId>,
}

impl BatteryWidget {
    pub fn new() -> Self {
        let icon = Image::builder().build();
        icon.add_css_class("battery-icon");
        let label = Label::new(None);

        let content_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(4)
            .build();
        content_box.append(&icon);
        content_box.append(&label);

        let container = Button::builder()
            .child(&content_box)
            .halign(Align::Center)
            .valign(Align::Center)
            .build();
        container.add_css_class("battery-button");

        let initial_state = Self::initialize_battery_state();
        let state_rc = Rc::new(RefCell::new(initial_state));

        let mut widget = Self {
            container,
            icon,
            label,
            state: state_rc,
            _update_source_id: None,
        };

        widget.update_widget();
        widget.schedule_update();

        widget
    }

    fn initialize_battery_state() -> BatteryState {
        match BatteryService::new() {
            Ok(service) => BatteryState {
                service: Some(service),
            },
            Err(e) => {
                eprintln!("Failed to create battery service: {}", e);
                BatteryState { service: None }
            }
        }
    }

    fn get_battery_info(state: &mut BatteryState) -> Result<BatteryDetails, BatteryUtilError> {
        let service = state
            .service
            .as_mut()
            .ok_or(BatteryUtilError::NoBatteryFound)?;
        service.get_primary_battery_details()
    }

    fn update_widget_internal(&self, _context: &str) {
        let mut state = self.state.borrow_mut();
        match Self::get_battery_info(&mut state) {
            Ok(info) => {
                let icon_name = info
                    .icon_name
                    .as_deref()
                    .unwrap_or("battery-missing-symbolic");
                self.icon.set_icon_name(Some(icon_name));
                let percentage_text = format!("{:.0}%", info.percentage.unwrap_or(0.0));
                self.label.set_text(&percentage_text);
                self.container.set_visible(true);

                let status_text = format_charge_status(&info);
                let tooltip = format!("{} - {}", percentage_text, status_text);
                self.container.set_tooltip_text(Some(&tooltip));
            }
            Err(e) => {
                self.icon.set_icon_name(Some("battery-missing-symbolic"));
                self.label.set_text("");
                self.container.set_visible(true);
                self.container
                    .set_tooltip_text(Some("Error reading battery"));
                if matches!(e, BatteryUtilError::NoBatteryFound) {
                    self.container.set_visible(false);
                }
            }
        }
    }

    fn update_widget(&self) {
        self.update_widget_internal("Initial Update");
    }

    fn schedule_update(&mut self) {
        let state_clone = self.state.clone();
        let container_clone = self.container.clone();
        let icon_clone = self.icon.clone();
        let label_clone = self.label.clone();

        let source_id = glib::timeout_add_local(REFRESH_INTERVAL, move || {
            let mut state = state_clone.borrow_mut();
            match Self::get_battery_info(&mut state) {
                Ok(info) => {
                    let icon_name = info
                        .icon_name
                        .as_deref()
                        .unwrap_or("battery-missing-symbolic");
                    icon_clone.set_icon_name(Some(icon_name));
                    let percentage_text = format!("{:.0}%", info.percentage.unwrap_or(0.0));
                    label_clone.set_text(&percentage_text);
                    container_clone.set_visible(true);

                    let status_text = format_charge_status(&info);
                    let tooltip = format!("{} - {}", percentage_text, status_text);
                    container_clone.set_tooltip_text(Some(&tooltip));
                }
                Err(e) => {
                    icon_clone.set_icon_name(Some("battery-missing-symbolic"));
                    label_clone.set_text("");
                    container_clone.set_visible(true);
                    container_clone.set_tooltip_text(Some("Error reading battery"));
                    if matches!(e, BatteryUtilError::NoBatteryFound) {
                        container_clone.set_visible(false);
                    }
                }
            }
            glib::ControlFlow::Continue
        });
        self._update_source_id = Some(source_id);
    }

    pub fn widget(&self) -> &Button {
        &self.container
    }
}

impl Default for BatteryWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BatteryWidget {
    fn drop(&mut self) {
        if let Some(source_id) = self._update_source_id.take() {
            source_id.remove();
        }
    }
}
