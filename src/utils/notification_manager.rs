use crate::utils::notification::Notification;
use crate::utils::notification_server::NotificationServer;
use crate::windows::{NotificationPopup, PopupCommand};
use gtk4::{glib, prelude::*, Application};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use tokio::sync::mpsc::{Receiver, Sender};

const BASE_MARGIN_TOP: i32 = 20;
const SPACING: i32 = 10;

pub struct NotificationManager {
    app: Application,
    command_tx: Sender<PopupCommand>,
    popups: HashMap<u32, NotificationPopup>,
    server: Arc<NotificationServer>,
    popup_order: Vec<u32>,
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
        }
    }

    fn display_notification(this_rc: Rc<RefCell<Self>>, n: Notification) {
        let mut me = this_rc.borrow_mut();
        let id = n.id;
        let rid = n.replaces_id;

        if rid != 0 {
            me.popups.remove(&rid);
            me.popup_order.retain(|&x| x != rid);
        }
        me.popups.remove(&id);
        me.popup_order.retain(|&x| x != id);

        let popup = NotificationPopup::new(&me.app, &n, me.command_tx.clone(), BASE_MARGIN_TOP);
        let window = popup.window().clone();
        me.popups.insert(id, popup);
        me.popup_order.push(id);
        drop(me);

        let rc_clone = this_rc.clone();
        window.connect_realize(move |_| {
            if let Ok(mut m) = rc_clone.try_borrow_mut() {
                let order = m.popup_order.clone();
                let mut y = BASE_MARGIN_TOP;
                for pid in order {
                    if let Some(p) = m.popups.get_mut(&pid) {
                        p.set_vertical_position(y);
                        let h = p.window().allocated_height().max(1);
                        y += h + SPACING;
                    }
                }
            }
        });

        window.present();
    }

    fn handle_popup_command(this_rc: Rc<RefCell<Self>>, cmd: PopupCommand) {
        let mut me = this_rc.borrow_mut();
        match cmd {
            PopupCommand::Close(id) => {
                me.popups.remove(&id).map(|mut p| p.close_popup());
                me.popup_order.retain(|&x| x != id);
                let srv = me.server.clone();
                glib::MainContext::default().spawn_local(async move {
                    let _ = srv.emit_notification_closed(id, 1).await;
                });
            }
            PopupCommand::ActionInvoked(id, key) => {
                let srv = me.server.clone();
                glib::MainContext::default().spawn_local(async move {
                    let _ = srv.emit_action_invoked(id, &key).await;
                    let _ = srv.emit_notification_closed(id, 2).await;
                });
                me.popups.remove(&id).map(|mut p| p.close_popup());
                me.popup_order.retain(|&x| x != id);
            }
        }
        drop(me);

        if let Ok(mut m) = this_rc.try_borrow_mut() {
            let order = m.popup_order.clone();
            let mut y = BASE_MARGIN_TOP;
            for pid in order {
                if let Some(p) = m.popups.get_mut(&pid) {
                    p.set_vertical_position(y);
                    let h = p.window().allocated_height().max(1);
                    y += h + SPACING;
                }
            }
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
) {
    let mgr = NotificationManager::new(app, tx_c, server);
    let rc = Rc::new(RefCell::new(mgr));
    glib::MainContext::default().spawn_local(async move {
        NotificationManager::run(rc, rx_n, rx_c).await;
    });
}
