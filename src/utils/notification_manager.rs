use crate::utils::{
    load_notifications, notification::Notification, notification_server::NotificationServer,
    save_notifications, BarConfig,
};
use crate::windows::{NotificationPopup, PopupCommand};
use gtk4::{glib, prelude::*, Application};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task;

const BASE_MARGIN_TOP: i32 = 10;
const SPACING: i32 = 10;

pub struct NotificationManager {
    app: Application,
    command_tx: Sender<PopupCommand>,
    popups: HashMap<u32, NotificationPopup>,
    server: Arc<NotificationServer>,
    popup_order: Vec<u32>,
    history: Vec<Notification>,
    config: BarConfig,
}

impl NotificationManager {
    pub fn new(
        app: Application,
        command_tx: Sender<PopupCommand>,
        server: Arc<NotificationServer>,
        config: BarConfig,
    ) -> Self {
        let history = match load_notifications() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to load notification history: {}", e);
                Vec::new()
            }
        };

        Self {
            app,
            command_tx,
            popups: HashMap::new(),
            server,
            popup_order: Vec::new(),
            history,
            config,
        }
    }

    fn save_history_async(history: Vec<Notification>) {
        task::spawn_blocking(move || {
            if let Err(e) = save_notifications(&history) {
                eprintln!("Failed to save notification history: {}", e);
            }
        });
    }

    fn recalculate_positions(&mut self) {
        let mut y = BASE_MARGIN_TOP;
        for id in &self.popup_order {
            if let Some(popup) = self.popups.get_mut(id) {
                popup.set_vertical_position(y);
                let height = popup.window().allocated_height();
                if height > 0 {
                    y += height + SPACING;
                } else {
                    let default_height = 100;
                    y += default_height + SPACING;
                }
            }
        }
    }

    fn display_notification(this_rc: Rc<RefCell<Self>>, n: Notification) {
        let config_clone;
        {
            let me = this_rc.borrow();
            config_clone = me.config.clone();
        }

        let mut me = this_rc.borrow_mut();
        let id = n.id;
        let rid = n.replaces_id;

        let mut replaced_existing = false;
        if rid != 0 {
            if let Some(mut old_popup) = me.popups.remove(&rid) {
                old_popup.close_popup();
            }
            me.popup_order.retain(|&x| x != rid);
            if let Some(index) = me.history.iter().position(|hist_n| hist_n.id == rid) {
                me.history.remove(index);
            }
        }

        if let Some(mut existing_popup) = me.popups.remove(&id) {
            existing_popup.close_popup();
            replaced_existing = true;
        }
        me.popup_order.retain(|&x| x != id);
        if let Some(index) = me.history.iter().position(|hist_n| hist_n.id == id) {
            me.history.remove(index);
            replaced_existing = true;
        }

        me.history.push(n.clone());
        Self::save_history_async(me.history.clone());

        let popup = NotificationPopup::new(
            &me.app,
            &n,
            me.command_tx.clone(),
            BASE_MARGIN_TOP,
            &config_clone,
        );
        let window = popup.window().clone();

        me.popups.insert(id, popup);

        if !replaced_existing {
            me.popup_order.push(id);
        } else {
            if me.popup_order.iter().position(|&x| x == id).is_none() {
                me.popup_order.push(id);
            }
        }

        me.recalculate_positions();
        drop(me);

        let rc_clone = this_rc.clone();
        window.connect_realize(move |_| {
            if let Ok(mut m) = rc_clone.try_borrow_mut() {
                m.recalculate_positions();
            }
        });

        window.present();
    }

    fn handle_popup_command(this_rc: Rc<RefCell<Self>>, cmd: PopupCommand) {
        let mut me = this_rc.borrow_mut();
        let mut needs_recalc = false;
        let mut closed_id = None;
        let mut reason = 0;

        match cmd {
            PopupCommand::Close(id) => {
                if let Some(mut p) = me.popups.remove(&id) {
                    p.close_popup();
                    me.popup_order.retain(|&x| x != id);
                    needs_recalc = true;
                    closed_id = Some(id);
                    reason = 1;
                }
            }
            PopupCommand::ActionInvoked(id, key) => {
                let srv = me.server.clone();
                glib::MainContext::default().spawn_local(async move {
                    let _ = srv.emit_action_invoked(id, &key).await;
                });

                if let Some(mut p) = me.popups.remove(&id) {
                    p.close_popup();
                    me.popup_order.retain(|&x| x != id);
                    needs_recalc = true;
                    closed_id = Some(id);
                    reason = 2;
                }
            }
        }

        if let Some(id_to_close) = closed_id {
            if let Some(index) = me
                .history
                .iter()
                .position(|hist_n| hist_n.id == id_to_close)
            {
                me.history.remove(index);
                Self::save_history_async(me.history.clone());
            }

            let srv = me.server.clone();
            glib::MainContext::default().spawn_local(async move {
                let _ = srv.emit_notification_closed(id_to_close, reason).await;
            });
        }

        if needs_recalc {
            me.recalculate_positions();
        }
    }

    pub async fn run(
        this_rc: Rc<RefCell<Self>>,
        mut rx_n: Receiver<Notification>,
        mut rx_c: Receiver<PopupCommand>,
    ) {
        loop {
            tokio::select! {
                Some(n) = rx_n.recv() => Self::display_notification(this_rc.clone(), n),
                Some(c) = rx_c.recv() => Self::handle_popup_command(this_rc.clone(), c),
                else => break,
            }
        }
    }
}

pub fn run_manager_task(
    app: Application,
    rx_n: Receiver<Notification>,
    tx_c: Sender<PopupCommand>,
    rx_c: Receiver<PopupCommand>,
    server: Arc<NotificationServer>,
    config: BarConfig,
) {
    let mgr = NotificationManager::new(app, tx_c, server, config);
    let rc = Rc::new(RefCell::new(mgr));
    glib::MainContext::default().spawn_local(async move {
        NotificationManager::run(rc, rx_n, rx_c).await;
    });
}
