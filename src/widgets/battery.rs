use crate::utils::battery::{BatteryService, BatteryUtilError};
use gtk4::prelude::*;
use gtk4::{glib, Align, Box as GtkBox, Button, Image, Label, Orientation};
use std::{cell::RefCell, rc::Rc, time::Duration};

const REFRESH_INTERVAL: Duration = Duration::from_secs(15);

pub struct BatteryWidget {
    container: Button,
    icon: Image,
    label: Label,
    _service: Rc<RefCell<BatteryService>>,
    _update_source_id: RefCell<Option<glib::SourceId>>,
}

impl BatteryWidget {
    pub fn new(service: Rc<RefCell<BatteryService>>) -> Rc<Self> {
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

        let widget = Rc::new(Self {
            container,
            icon,
            label,
            _service: service.clone(),
            _update_source_id: RefCell::new(None),
        });

        let widget_clone = widget.clone();
        widget.container.connect_destroy(move |_| {
            let _ = &widget_clone;
        });

        let weak_self = Rc::downgrade(&widget);
        let service_clone = service;

        let update_ui = move |strong_self: &BatteryWidget| {
            let result = {
                let mut service_borrow = service_clone.borrow_mut();
                service_borrow.get_primary_battery_details()
            };

            match result {
                Ok(info) => {
                    let icon_name = info
                        .icon_name
                        .as_deref()
                        .unwrap_or("battery-missing-symbolic");
                    strong_self.icon.set_icon_name(Some(icon_name));
                    let percentage_text = format!("{:.0}%", info.percentage.unwrap_or(0.0));
                    strong_self.label.set_text(&percentage_text);
                    strong_self.container.set_visible(true);
                }
                Err(e) => {
                    strong_self
                        .icon
                        .set_icon_name(Some("battery-missing-symbolic"));
                    strong_self.label.set_text("");
                    strong_self.container.set_visible(true);
                    if matches!(e, BatteryUtilError::NoBatteryFound) {
                        strong_self.container.set_visible(false);
                    }
                }
            }
            strong_self.container.queue_draw();
        };

        update_ui(&widget);

        let source_id =
            glib::timeout_add_local(REFRESH_INTERVAL, move || match weak_self.upgrade() {
                Some(strong_self) => {
                    update_ui(&strong_self);
                    glib::ControlFlow::Continue
                }
                None => glib::ControlFlow::Break,
            });
        *widget._update_source_id.borrow_mut() = Some(source_id);

        widget
    }

    pub fn widget(&self) -> &Button {
        &self.container
    }
}

impl Default for BatteryWidget {
    fn default() -> Self {
        panic!("BatteryWidget::default() is not supported. Use BatteryWidget::new(service).");
    }
}

impl Drop for BatteryWidget {
    fn drop(&mut self) {
        if let Some(source_id) = self._update_source_id.borrow_mut().take() {
            source_id.remove();
        }
    }
}
