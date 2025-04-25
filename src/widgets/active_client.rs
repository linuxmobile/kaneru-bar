use crate::utils::niri::{self, NiriError, Window};
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

    fn update_labels(&self, window_info: Result<Option<Window>, NiriError>) {
        match window_info {
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
            Err(_e) => {
                self.app_id_label.set_text("");
                self.title_label.set_text("Error");
                self.container.set_visible(false);
            }
        }
    }

    fn update_widget_info(&self) {
        let window_info = niri::get_focused_window();
        self.update_labels(window_info);
    }

    fn schedule_update(&self) {
        let container = self.container.clone();
        let app_id_label = self.app_id_label.clone();
        let title_label = self.title_label.clone();

        glib::timeout_add_local(UPDATE_INTERVAL, move || {
            let update_result = niri::get_focused_window();

            match update_result {
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
                Err(_e) => {
                    app_id_label.set_text("");
                    title_label.set_text("Error");
                    container.set_visible(false);
                }
            }
            glib::ControlFlow::Continue
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
