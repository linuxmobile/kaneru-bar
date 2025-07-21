use crate::utils::{Notification, NotificationPosition, Urgency};
use gtk4::prelude::*;
use gtk4::{
    glib, Align, ApplicationWindow, Box, Button, EventControllerMotion, Image, Justification,
    Label, Orientation, Revealer, RevealerTransitionType,
};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::{cell::RefCell, rc::Rc, time::Duration};
use tokio::sync::mpsc;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const CRITICAL_TIMEOUT: Duration = Duration::from_secs(10);
const POPUP_WINDOW_CLASS: &str = "notification-popup-window";
const POPUP_MARGIN: i32 = 10;

#[derive(Debug)]
pub enum PopupCommand {
    Close(u32),
    ActionInvoked(u32, String),
}

pub struct NotificationPopup {
    window: ApplicationWindow,
    notification_id: u32,
    command_sender: mpsc::Sender<PopupCommand>,
    close_timer_source_id: Rc<RefCell<Option<glib::SourceId>>>,
    vertical_position: i32,
    is_closing: Rc<RefCell<bool>>,
    position_edge: Edge,
}

impl NotificationPopup {
    pub fn new(
        app: &gtk4::Application,
        notification: &Notification,
        command_sender: mpsc::Sender<PopupCommand>,
        initial_vertical_position: i32,
        config: &crate::utils::BarConfig,
    ) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .decorated(false)
            .resizable(false)
            .build();

        window.add_css_class(POPUP_WINDOW_CLASS);

        window.init_layer_shell();
        window.set_layer(Layer::Top);

        let position = config.notification_position;
        let position_edge = match position {
            NotificationPosition::TopLeft => {
                window.set_anchor(Edge::Top, true);
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Bottom, false);
                window.set_anchor(Edge::Right, false);
                window.set_margin(Edge::Left, POPUP_MARGIN);
                Edge::Top
            }
            NotificationPosition::TopRight => {
                window.set_anchor(Edge::Top, true);
                window.set_anchor(Edge::Right, true);
                window.set_anchor(Edge::Bottom, false);
                window.set_anchor(Edge::Left, false);
                window.set_margin(Edge::Right, POPUP_MARGIN);
                Edge::Top
            }
            NotificationPosition::BottomLeft => {
                window.set_anchor(Edge::Bottom, true);
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Top, false);
                window.set_anchor(Edge::Right, false);
                window.set_margin(Edge::Left, POPUP_MARGIN);
                Edge::Bottom
            }
            NotificationPosition::BottomRight => {
                window.set_anchor(Edge::Bottom, true);
                window.set_anchor(Edge::Right, true);
                window.set_anchor(Edge::Top, false);
                window.set_anchor(Edge::Left, false);
                window.set_margin(Edge::Right, POPUP_MARGIN);
                Edge::Bottom
            }
        };

        window.set_margin(position_edge, initial_vertical_position);
        window.set_namespace(Some("kaneru-notification-popup"));

        if notification.urgency == Urgency::Critical {
            window.add_css_class("critical");
        }

        let notification_id = notification.id;
        let is_closing = Rc::new(RefCell::new(false));

        let main_box = Box::builder().orientation(Orientation::Vertical).build();
        main_box.add_css_class("notification-content-box");

        let header_box = Box::builder().orientation(Orientation::Horizontal).build();
        header_box.add_css_class("header");

        let icon = if notification.app_icon.is_empty() {
            Image::builder()
                .icon_name("dialog-information-symbolic")
                .pixel_size(18)
                .build()
        } else {
            Image::builder()
                .icon_name(&notification.app_icon)
                .pixel_size(18)
                .build()
        };
        icon.add_css_class("app-icon");
        header_box.append(&icon);

        let app_name_label = Label::builder()
            .label(&notification.app_name)
            .halign(Align::Start)
            .hexpand(true)
            .xalign(0.0)
            .build();
        app_name_label.add_css_class("app-name");
        header_box.append(&app_name_label);

        let close_button = Button::from_icon_name("window-close-symbolic");
        close_button.add_css_class("close-button");
        let sender_clone = command_sender.clone();
        let id_clone = notification_id;
        let is_closing_clone = is_closing.clone();
        close_button.connect_clicked(move |_| {
    if *is_closing_clone.borrow() {
        eprintln!("[NotificationPopup] Close button clicked, but already closing (id={})", id_clone);
        return;
    }
    *is_closing_clone.borrow_mut() = true;
    let sender = sender_clone.clone();
    let id = id_clone;
    eprintln!("[NotificationPopup] Close button clicked, sending PopupCommand::Close({})", id);
    glib::MainContext::default().spawn_local(async move {
        if let Err(e) = sender.send(PopupCommand::Close(id)).await {
            eprintln!("[NotificationPopup] Failed to send PopupCommand::Close({}): {}", id, e);
        }
    });
});
        header_box.append(&close_button);

        main_box.append(&header_box);

        let content_box = Box::builder().orientation(Orientation::Vertical).build();
        content_box.add_css_class("content");

        let summary_label = Label::builder()
            .label(&notification.summary)
            .halign(Align::Start)
            .xalign(0.0)
            .wrap(true)
            .justify(Justification::Left)
            .build();
        summary_label.add_css_class("summary");
        content_box.append(&summary_label);

        if !notification.body.is_empty() {
            let body_label = Label::builder()
                .label(&notification.body)
                .halign(Align::Start)
                .xalign(0.0)
                .wrap(true)
                .use_markup(true)
                .justify(Justification::Left)
                .build();
            body_label.add_css_class("body");
            content_box.append(&body_label);
        }

        if let Some(image_path) = &notification.image_path {
            let file = gtk4::gio::File::for_path(image_path);
            let cancellable: Option<&gtk4::gio::Cancellable> = None;

            if file.query_exists(cancellable) {
                let image_box = Box::new(Orientation::Horizontal, 0);
                let image = Image::from_file(image_path);
                image.add_css_class("image");
                image.set_halign(Align::Start);
                image_box.append(&image);
                content_box.append(&image_box);
            } else {
            }
        }

        main_box.append(&content_box);

        if !notification.actions.is_empty() {
            let actions_box = Box::builder()
                .orientation(Orientation::Horizontal)
                .halign(Align::End)
                .spacing(6)
                .build();
            actions_box.add_css_class("actions");

            for chunk in notification.actions.chunks_exact(2) {
                if chunk.len() == 2 {
                    let key = chunk[0].to_string();
                    let label = chunk[1].to_string();

                    let action_button = Button::with_label(&label);
                    action_button.add_css_class("action-button");
                    let sender_for_action = command_sender.clone();
                    let key_clone = key.clone();
                    let id_clone = notification_id;
                    let is_closing_clone_action = is_closing.clone();
                    action_button.connect_clicked(move |_| {
                        if *is_closing_clone_action.borrow() {
                            return;
                        }
                        *is_closing_clone_action.borrow_mut() = true;
                        let sender = sender_for_action.clone();
                        let key_for_send = key_clone.clone();
                        let id = id_clone;
                        glib::MainContext::default().spawn_local(async move {
                            if let Err(_e) = sender
                                .send(PopupCommand::ActionInvoked(id, key_for_send))
                                .await
                            {}
                        });
                    });
                    actions_box.append(&action_button);
                }
            }
            main_box.append(&actions_box);
        }

        let revealer = Revealer::builder()
            .transition_type(match position {
                NotificationPosition::TopLeft | NotificationPosition::TopRight => {
                    RevealerTransitionType::SlideDown
                }
                NotificationPosition::BottomLeft | NotificationPosition::BottomRight => {
                    RevealerTransitionType::SlideUp
                }
            })
            .transition_duration(250)
            .child(&main_box)
            .reveal_child(false)
            .build();

        window.set_child(Some(&revealer));

        let window_clone_for_idle = window.clone();
        glib::idle_add_local_once(move || {
            if let Some(rev) = window_clone_for_idle
                .child()
                .and_then(|c| c.downcast::<Revealer>().ok())
            {
                rev.set_reveal_child(true);
            }
        });

        let close_timer_source_id = Rc::new(RefCell::new(None));

        let mut popup = Self {
            window,
            notification_id,
            command_sender: command_sender.clone(),
            close_timer_source_id: close_timer_source_id.clone(),
            vertical_position: initial_vertical_position,
            is_closing: is_closing.clone(),
            position_edge,
        };

        popup.reset_close_timer(notification);

        let motion_controller = EventControllerMotion::new();

        let timer_rc_enter = popup.close_timer_source_id.clone();
        motion_controller.connect_enter(move |_, _, _| {
            if let Some(timer_id) = timer_rc_enter.borrow_mut().take() {
                timer_id.remove();
            }
        });

        let timer_rc_leave = popup.close_timer_source_id.clone();
        let sender_leave = popup.command_sender.clone();
        let id_leave = popup.notification_id;
        let is_closing_leave = popup.is_closing.clone();
        let urgency_leave = notification.urgency;
        let resident_leave = notification.resident;

        motion_controller.connect_leave(move |_| {
            if *is_closing_leave.borrow() {
                return;
            }
            if timer_rc_leave.borrow().is_none() {
                let sender = sender_leave.clone();
                let id = id_leave;
                let timer_id_rc = timer_rc_leave.clone();
                let is_closing_rc = is_closing_leave.clone();

                let duration = if resident_leave {
                    None
                } else {
                    Some(match urgency_leave {
                        Urgency::Critical => CRITICAL_TIMEOUT,
                        _ => DEFAULT_TIMEOUT,
                    })
                };

                if let Some(d) = duration {
                    let source_id = glib::timeout_add_local_once(d, move || {
                        if *is_closing_rc.borrow() {
                            return;
                        }
                        timer_id_rc.borrow_mut().take();
                        let sender_clone = sender.clone();
                        glib::MainContext::default().spawn_local(async move {
                            if let Err(_e) = sender_clone.send(PopupCommand::Close(id)).await {}
                        });
                    });
                    *timer_rc_leave.borrow_mut() = Some(source_id);
                }
            }
        });

        popup.window.add_controller(motion_controller);

        popup
    }

    pub fn window(&self) -> &ApplicationWindow {
        &self.window
    }

    pub fn set_vertical_position(&mut self, position: i32) {
        if self.vertical_position != position {
            self.vertical_position = position;
            self.window.set_margin(self.position_edge, position);
        }
    }

    pub fn close_popup(&mut self) {
    if *self.is_closing.borrow() {
        eprintln!("[NotificationPopup] close_popup called, but already closing (id={})", self.notification_id);
        return;
    }
    *self.is_closing.borrow_mut() = true;
    eprintln!("[NotificationPopup] close_popup called for id={}", self.notification_id);

    if let Some(source_id) = self.close_timer_source_id.borrow_mut().take() {
        let _ = source_id.remove();
        eprintln!("[NotificationPopup] Removed close timer for id={}", self.notification_id);
    }

    let mut destroyed = false;
    if let Some(revealer) = self
        .window
        .child()
        .and_then(|w| w.downcast::<Revealer>().ok())
    {
        if revealer.reveals_child() {
            revealer.set_reveal_child(false);
let window_clone = self.window.clone();
let transition_duration = revealer.transition_duration();
let id = self.notification_id;
eprintln!("[NotificationPopup] Starting slide-out animation for id={}, duration={}ms", id, transition_duration);
glib::timeout_add_local_once(
    Duration::from_millis(transition_duration as u64 + 50),
    move || {
        eprintln!("[NotificationPopup] Destroying window after animation (id={})", id);
        window_clone.destroy();
    },
);            destroyed = true;
        }
    }
    if !destroyed {
        eprintln!("[NotificationPopup] Destroying window immediately (id={})", self.notification_id);
        self.window.destroy();
    }

    // Fallback: force destroy after 1s if not already destroyed
    let window_clone = self.window.clone();
    let id = self.notification_id;
    glib::timeout_add_local_once(Duration::from_secs(1), move || {
        if window_clone.is_visible() {
            eprintln!("[NotificationPopup] Fallback: Forcing window destroy after 1s (id={})", id);
            window_clone.destroy();
        }
    });
}


    fn reset_close_timer(&mut self, notification: &Notification) {
        if let Some(source_id) = self.close_timer_source_id.borrow_mut().take() {
            let _ = source_id.remove();
        }

        let timeout_ms = notification.expire_timeout;
        let duration = match timeout_ms {
            0 => None,
            -1 => {
                if notification.resident {
                    None
                } else {
                    match notification.urgency {
                        Urgency::Critical => Some(CRITICAL_TIMEOUT),
                        _ => Some(DEFAULT_TIMEOUT),
                    }
                }
            }
            ms if ms > 0 => Some(Duration::from_millis(ms as u64)),
            _ => Some(DEFAULT_TIMEOUT),
        };

        if let Some(d) = duration {
            let sender = self.command_sender.clone();
            let id = self.notification_id;
            let timer_id_rc = self.close_timer_source_id.clone();
            let is_closing_rc = self.is_closing.clone();

            let source_id = glib::timeout_add_local_once(d, move || {
                if *is_closing_rc.borrow() {
                    return;
                }
                timer_id_rc.borrow_mut().take();
                let sender_clone = sender.clone();
                glib::MainContext::default().spawn_local(async move {
                    if let Err(_e) = sender_clone.send(PopupCommand::Close(id)).await {}
                });
            });
            *self.close_timer_source_id.borrow_mut() = Some(source_id);
        }
    }
}

impl Drop for NotificationPopup {
    fn drop(&mut self) {
        if let Some(source_id) = self.close_timer_source_id.borrow_mut().take() {
            let _ = source_id.remove();
        }
    }
}
