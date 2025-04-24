use crate::utils::niri;
use gtk4::prelude::*;
use gtk4::{Align, Box, Label, Orientation, Widget};
use pango::EllipsizeMode;
use std::time::Duration;

const UPDATE_INTERVAL: Duration = Duration::from_secs(1);

pub struct ActiveClientWidget {
    container: Box,
}

impl ActiveClientWidget {
    pub fn new() -> Self {
        let app_id_label = Label::builder()
            .halign(Align::Start)
            .xalign(0.0)
            .ellipsize(EllipsizeMode::End)
            .build();
        app_id_label.add_css_class("app-id");

        let title_label = Label::builder()
            .halign(Align::Start)
            .xalign(0.0)
            .ellipsize(EllipsizeMode::End)
            .build();
        title_label.add_css_class("window-title");

        let container = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .build();
        container.add_css_class("ActiveClient");

        container.append(&app_id_label);
        container.append(&title_label);

        Self::update_widget_info(&container, &app_id_label, &title_label);
        Self::schedule_update(container.clone(), app_id_label.clone(), title_label.clone());

        Self { container }
    }

    fn update_widget_info(_container: &Box, app_id_label: &Label, title_label: &Label) {
        match niri::get_focused_window() {
            Ok(Some(window)) => {
                app_id_label.set_text(&window.app_id.unwrap_or_default());
                title_label.set_text(&window.title.unwrap_or_default());
            }
            Ok(None) => {
                app_id_label.set_text("niri");
                title_label.set_text("Desktop");
            }
            Err(e) => {
                eprintln!("Failed to get focused window info: {}", e);
                app_id_label.set_text("niri");
                title_label.set_text("Error");
            }
        }
    }

    fn schedule_update(container: Box, app_id_label: Label, title_label: Label) {
        let container_weak = container.downgrade();
        let app_id_label_weak = app_id_label.downgrade();
        let title_label_weak = title_label.downgrade();

        glib::timeout_add_local(UPDATE_INTERVAL, move || {
            if let (Some(container), Some(app_id_label), Some(title_label)) = (
                container_weak.upgrade(),
                app_id_label_weak.upgrade(),
                title_label_weak.upgrade(),
            ) {
                Self::update_widget_info(&container, &app_id_label, &title_label);
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }

    pub fn widget(&self) -> &impl IsA<Widget> {
        &self.container
    }
}

impl Default for ActiveClientWidget {
    fn default() -> Self {
        Self::new()
    }
}
