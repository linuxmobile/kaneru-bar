use crate::utils::network::{NetworkService, NetworkUtilError, WifiDetails};
use gtk4::prelude::*;
use gtk4::{glib, Align, Box as GtkBox, Button, Image, Orientation};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use tokio::sync::Mutex;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

pub struct NetworkWidget {
    container: Button,
    icon: Image,
    service: Arc<NetworkService>,
    details: Arc<Mutex<Option<WifiDetails>>>,
    _update_source_id: RefCell<Option<glib::SourceId>>,
}

impl NetworkWidget {
    pub fn new(service: Arc<NetworkService>) -> Rc<Self> {
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
            service: service.clone(),
            details: Arc::new(Mutex::new(None)),
            _update_source_id: RefCell::new(None),
        });

        let widget_clone = widget.clone();
        widget.container.connect_destroy(move |_| {
            let _ = &widget_clone;
        });

        let weak_self = Rc::downgrade(&widget);

        let weak_self_init = weak_self.clone();
        glib::MainContext::default().spawn_local(async move {
            if let Some(s) = weak_self_init.upgrade() {
                s.update_network_details(true).await;
            }
        });

        let source_id = glib::timeout_add_local(REFRESH_INTERVAL, move || {
            if let Some(strong_self) = weak_self.upgrade() {
                let s_clone = strong_self.clone();
                glib::MainContext::default().spawn_local(async move {
                    s_clone.update_network_details(false).await;
                });
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        *widget._update_source_id.borrow_mut() = Some(source_id);

        widget
    }

    async fn update_network_details(self: &Rc<Self>, is_initial: bool) {
        match self.service.get_wifi_details().await {
            Ok(d) => {
                let mut details_guard = self.details.lock().await;
                *details_guard = Some(d);
                self.update_ui_from_details(&details_guard);
            }
            Err(e) => {
                if is_initial {
                    eprintln!("Initial network fetch failed: {}", e);
                    self.set_error_state();
                } else {
                    println!("Network update failed: {}", e);
                }
                if matches!(e, NetworkUtilError::NoWifiDevice) {
                    self.container.set_visible(false);
                } else {
                    self.container.set_visible(true);
                }
            }
        }
    }

    fn update_ui_from_details(&self, details_opt: &Option<WifiDetails>) {
        if let Some(details) = details_opt {
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
