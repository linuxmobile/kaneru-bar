use crate::utils::network::{NetworkCommand, WifiDetails};
use gtk4::prelude::*;
use gtk4::{glib, Align, Box as GtkBox, Button, Image, Orientation};
use std::{cell::RefCell, rc::Rc, time::Duration};
use tokio::sync::mpsc;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

pub struct NetworkWidget {
    container: Button,
    icon: Image,
    command_sender: mpsc::Sender<NetworkCommand>,
    details: Rc<RefCell<Option<WifiDetails>>>,
    _update_source_id: Rc<RefCell<Option<glib::SourceId>>>,
}

impl NetworkWidget {
    pub fn new(command_sender: mpsc::Sender<NetworkCommand>) -> Rc<Self> {
        let icon = Image::builder()
            .icon_name("network-wireless-offline-symbolic")
            .build();
        icon.add_css_class("network-icon");

        let content_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(0)
            .build();
        content_box.append(&icon);

        let container = Button::builder()
            .child(&content_box)
            .halign(Align::Center)
            .valign(Align::Center)
            .build();
        container.add_css_class("wifi-button");

        let widget = Rc::new(Self {
            container,
            icon,
            command_sender: command_sender.clone(),
            details: Rc::new(RefCell::new(None)),
            _update_source_id: Rc::new(RefCell::new(None)),
        });

        let widget_clone = widget.clone();
        widget.container.connect_destroy(move |_| {
            let _ = &widget_clone;
        });

        Self::schedule_initial_update(&widget);
        Self::schedule_periodic_updates(&widget);

        widget
    }

    fn schedule_initial_update(widget_rc: &Rc<Self>) {
        let sender = widget_rc.command_sender.clone();
        glib::MainContext::default().spawn_local(async move {
            let _ = sender.send(NetworkCommand::GetDetails).await;
        });
    }

    fn schedule_periodic_updates(widget_rc: &Rc<Self>) {
        let weak_self = Rc::downgrade(widget_rc);
        let update_source_id_clone = widget_rc._update_source_id.clone();
        let source_id = glib::timeout_add_local(REFRESH_INTERVAL, move || {
            if let Some(strong_self) = weak_self.upgrade() {
                let sender = strong_self.command_sender.clone();
                glib::MainContext::default().spawn_local(async move {
                    let _ = sender.send(NetworkCommand::GetDetails).await;
                });
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        *update_source_id_clone.borrow_mut() = Some(source_id);
    }

    pub fn update_state(
        &self,
        details_res: Result<WifiDetails, crate::utils::network::NetworkUtilError>,
    ) {
        match details_res {
            Ok(d) => {
                *self.details.borrow_mut() = Some(d);
                self.update_ui_from_state();
            }
            Err(e) => {
                eprintln!("[Widget] Failed to update network state: {}", e);
                *self.details.borrow_mut() = None;
                self.set_error_state();
                if matches!(e, crate::utils::network::NetworkUtilError::NoWifiDevice) {
                    self.container.set_visible(false);
                } else {
                    self.container.set_visible(true);
                }
            }
        }
    }

    fn update_ui_from_state(&self) {
        let details_opt = self.details.borrow();
        if let Some(details) = details_opt.as_ref() {
            self.icon.set_icon_name(Some(&details.icon_name));
            self.container.set_visible(true);
        } else {
            self.set_error_state();
        }
        self.container.queue_draw();
    }

    fn set_error_state(&self) {
        self.icon
            .set_icon_name(Some("network-wireless-offline-symbolic"));
        self.container.set_visible(true);
    }

    pub fn widget(&self) -> &Button {
        &self.container
    }
}

impl Drop for NetworkWidget {
    fn drop(&mut self) {
        if let Some(source_id) = self._update_source_id.borrow_mut().take() {
            source_id.remove();
        }
    }
}
