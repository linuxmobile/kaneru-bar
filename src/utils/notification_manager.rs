use crate::utils::notification::Notification;
use crate::utils::notification_server::{self, NotificationServer};
use crate::windows::{NotificationPopup, PopupCommand};
use gtk4::prelude::*;
use gtk4::{glib, Application};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};

const BASE_MARGIN_TOP: i32 = 20;
const NOTIFICATION_SPACING: i32 = 10;

pub struct NotificationManager {
    app: Application,
    command_tx: Sender<PopupCommand>,
    popups: HashMap<u32, NotificationPopup>,
    server: Arc<NotificationServer>,
    popup_order: Vec<u32>,
    reposition_scheduled: bool,
}

impl NotificationManager {
    pub fn new(
        app: Application,
        command_tx: Sender<PopupCommand>,
        server: Arc<NotificationServer>,
    ) -> Self {
        Self {
            app,
            command_tx,
            popups: HashMap::new(),
            server,
            popup_order: Vec::new(),
            reposition_scheduled: false,
        }
    }

    fn display_notification(manager_rc: Rc<RefCell<Self>>, notification: Notification) {
        let mut manager = manager_rc.borrow_mut();
        let id = notification.id;
        let replaces_id = notification.replaces_id;

        if replaces_id != 0 {
            if let Some(existing_popup) = manager.popups.get_mut(&replaces_id) {
                existing_popup.close_popup();
                manager.remove_popup_from_order(replaces_id);
            } else if replaces_id != id {
                if let Some(existing_popup) = manager.popups.get_mut(&id) {
                    existing_popup.close_popup();
                    manager.remove_popup_from_order(id);
                }
            }
        } else if let Some(existing_popup) = manager.popups.get_mut(&id) {
            existing_popup.close_popup();
            manager.remove_popup_from_order(id);
        }

        let initial_y = manager.calculate_next_vertical_position();

        let popup = NotificationPopup::new(
            &manager.app,
            &notification,
            manager.command_tx.clone(),
            initial_y,
        );

        popup.window().present();

        if manager.popups.insert(id, popup).is_some() {}
        manager.popup_order.push(id);

        drop(manager);
        Self::schedule_reposition(manager_rc);
    }

    fn handle_popup_command(manager_rc: Rc<RefCell<Self>>, command: PopupCommand) {
        let mut manager = manager_rc.borrow_mut();
        let mut needs_reposition = false;
        match command {
            PopupCommand::Close(id) => {
                if let Some(mut popup) = manager.popups.remove(&id) {
                    popup.close_popup();
                    if manager.remove_popup_from_order(id) {
                        needs_reposition = true;
                    }

                    let server = manager.server.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let reason = 1;
                        if let Err(_e) = notification_server::signals::emit_notification_closed(
                            &server, id, reason,
                        )
                        .await
                        {}
                    });
                } else {
                }
            }
            PopupCommand::ActionInvoked(id, key) => {
                let server = manager.server.clone();
                let key_clone = key.clone();

                glib::MainContext::default().spawn_local(async move {
                    if let Err(_e) =
                        notification_server::signals::emit_action_invoked(&server, id, &key_clone)
                            .await
                    {}

                    let reason = 2;
                    if let Err(_e) =
                        notification_server::signals::emit_notification_closed(&server, id, reason)
                            .await
                    {}
                });

                if let Some(mut popup) = manager.popups.remove(&id) {
                    popup.close_popup();
                    if manager.remove_popup_from_order(id) {
                        needs_reposition = true;
                    }
                } else {
                }
            }
        }
        if needs_reposition {
            manager.reposition_notifications();
        }
    }

    fn get_popup_height(popup: &NotificationPopup) -> i32 {
        popup.window().allocated_height()
    }

    fn calculate_next_vertical_position(&self) -> i32 {
        let mut position = BASE_MARGIN_TOP;
        for id in &self.popup_order {
            if let Some(popup) = self.popups.get(id) {
                let height = Self::get_popup_height(popup);
                position += height.max(1) + NOTIFICATION_SPACING;
            } else {
                position += 100 + NOTIFICATION_SPACING;
            }
        }
        position
    }

    fn remove_popup_from_order(&mut self, id: u32) -> bool {
        if let Some(pos) = self.popup_order.iter().position(|&x| x == id) {
            self.popup_order.remove(pos);
            true
        } else {
            false
        }
    }

    fn schedule_reposition(manager_rc: Rc<RefCell<Self>>) {
        let mut should_schedule = false;
        if let Ok(mut manager) = manager_rc.try_borrow_mut() {
            if !manager.reposition_scheduled {
                manager.reposition_scheduled = true;
                should_schedule = true;
            }
        } else {
            return;
        }

        if !should_schedule {
            return;
        }

        let manager_rc_clone = manager_rc.clone();
        glib::idle_add_local_once(move || {
            if let Ok(mut manager) = manager_rc_clone.try_borrow_mut() {
                manager.reposition_scheduled = false;
                manager.reposition_notifications();
            } else {
                eprintln!("Error: Failed to borrow manager mutably in idle_add_local_once for repositioning.");
                if let Ok(mut manager_reset) = manager_rc_clone.try_borrow_mut() {
                    manager_reset.reposition_scheduled = false;
                }
            }
        });
    }

    fn reposition_notifications(&mut self) {
        let mut positions_with_heights = Vec::with_capacity(self.popup_order.len());
        let mut current_y = BASE_MARGIN_TOP;

        for id in &self.popup_order {
            let height = if let Some(popup) = self.popups.get(id) {
                Self::get_popup_height(popup).max(1)
            } else {
                100
            };
            positions_with_heights.push((*id, current_y));
            current_y += height + NOTIFICATION_SPACING;
        }

        for (id, target_y) in positions_with_heights {
            if let Some(popup) = self.popups.get_mut(&id) {
                popup.set_vertical_position(target_y);
            }
        }
    }

    pub async fn run(
        manager_rc: Rc<RefCell<Self>>,
        mut notify_rx: Receiver<Notification>,
        mut command_rx: Receiver<PopupCommand>,
    ) {
        loop {
            tokio::select! {
                biased;

                Some(notification) = notify_rx.recv() => {
                    Self::display_notification(manager_rc.clone(), notification);
                }

                 Some(popup_cmd) = command_rx.recv() => {
                     Self::handle_popup_command(manager_rc.clone(), popup_cmd);
                 }

                else => {
                    break;
                }
            }
        }
    }
}

pub fn run_manager_task(
    app: Application,
    notify_rx: Receiver<Notification>,
    command_tx: Sender<PopupCommand>,
    command_rx: Receiver<PopupCommand>,
    server: Arc<NotificationServer>,
) {
    let manager = NotificationManager::new(app, command_tx, server);

    let manager_rc = Rc::new(RefCell::new(manager));

    let context = glib::MainContext::default();

    let manager_rc_clone = manager_rc.clone();
    context.spawn_local(async move {
        NotificationManager::run(manager_rc_clone, notify_rx, command_rx).await;
    });
}
