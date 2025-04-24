use crate::utils::niri;
use glib;
use gtk4::prelude::*;
use gtk4::{Align, Box, Label, Orientation, Widget};
use pango::EllipsizeMode;
use std::time::Duration;

const UPDATE_INTERVAL: Duration = Duration::from_secs(1);

pub struct ActiveClientWidget {
    container: Box,
    app_id_label: Label,
    title_label: Label,
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

        let widget = Self {
            container,
            app_id_label,
            title_label,
        };

        widget.update_widget_info();
        widget.schedule_update();

        widget
    }

    fn update_widget_info(&self) {
        match niri::get_focused_window() {
            Ok(Some(window)) => {
                self.app_id_label
                    .set_text(&window.app_id.unwrap_or_default());
                self.title_label.set_text(&window.title.unwrap_or_default());
                self.container.set_visible(true);
            }
            Ok(None) => {
                self.app_id_label.set_text("niri");
                self.title_label.set_text("Desktop");
                self.container.set_visible(true);
            }
            Err(e) => {
                eprintln!("Failed to get focused window info: {:?}", e);
                self.app_id_label.set_text("");
                self.title_label.set_text("Error");
                self.container.set_visible(false);
            }
        }
    }

    fn schedule_update(&self) {
        glib::timeout_add_local(
            UPDATE_INTERVAL,
            glib::clone!(@weak self.container as container, @weak self.app_id_label as app_id_label, @weak self.title_label as title_label => @default-return glib::ControlFlow::Break, move || {
                match niri::get_focused_window() {
                    Ok(Some(window)) => {
                        app_id_label.set_text(&window.app_id.unwrap_or_default());
                        title_label.set_text(&window.title.unwrap_or_default());
                        container.set_visible(true);
                    }
                    Ok(None) => {
                        app_id_label.set_text("niri");
                        title_label.set_text("Desktop");
                        container.set_visible(true);
                    }
                    Err(e) => {
                        eprintln!("Failed to get focused window info during update: {:?}", e);
                        app_id_label.set_text("");
                        title_label.set_text("Error");
                    }
                }
                glib::ControlFlow::Continue
            }),
        );
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
