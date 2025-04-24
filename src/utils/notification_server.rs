use crate::utils::notification::{Notification, Urgency};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::future::pending;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use zbus::zvariant::Value;
use zbus::{interface, Connection};

static NEXT_NOTIFICATION_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Clone)]
pub struct NotificationServer {
    notify_tx: mpsc::Sender<Notification>,
    active_notifications: Arc<Mutex<HashMap<u32, Notification>>>,
    connection: Arc<Mutex<Option<Connection>>>,
}

impl NotificationServer {
    pub fn new(notify_tx: mpsc::Sender<Notification>) -> Self {
        Self {
            notify_tx,
            active_notifications: Arc::new(Mutex::new(HashMap::new())),
            connection: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn set_connection(&self, conn: Connection) {
        let mut g = self.connection.lock().await;
        *g = Some(conn);
    }

    pub async fn emit_notification_closed(&self, id: u32, reason: u32) -> zbus::Result<()> {
        if let Some(c) = &*self.connection.lock().await {
            c.emit_signal(
                None::<()>,
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "NotificationClosed",
                &(id, reason),
            )
            .await
        } else {
            Err(zbus::Error::Failure("No D-Bus connection".into()))
        }
    }

    pub async fn emit_action_invoked(&self, id: u32, key: &str) -> zbus::Result<()> {
        if let Some(c) = &*self.connection.lock().await {
            c.emit_signal(
                None::<()>,
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "ActionInvoked",
                &(id, key),
            )
            .await
        } else {
            Err(zbus::Error::Failure("No D-Bus connection".into()))
        }
    }
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    #[zbus(name = "Notify")]
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints_raw: HashMap<String, Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::fdo::Result<u32> {
        let id = if replaces_id == 0 {
            NEXT_NOTIFICATION_ID.fetch_add(1, Ordering::Relaxed)
        } else {
            replaces_id
        };

        let mut owned = HashMap::new();
        for (k, v) in hints_raw {
            if let Ok(o) = v.try_to_owned() {
                owned.insert(k, o);
            }
        }

        let urgency = owned
            .get("urgency")
            .and_then(|v| Urgency::try_from(v.clone()).ok())
            .unwrap_or(Urgency::Normal);

        let image_path = owned
            .get("image-path")
            .and_then(|v| String::try_from(v.clone()).ok());

        let resident = owned
            .get("resident")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);

        let notification = Notification::new(
            id,
            app_name.clone(),
            replaces_id,
            app_icon.clone(),
            summary.clone(),
            body.clone(),
            actions.clone(),
            expire_timeout,
            urgency,
            image_path,
            resident,
        );

        {
            let mut m = self.active_notifications.lock().await;
            m.insert(id, notification.clone());
        }

        let _ = self.notify_tx.send(notification).await;
        Ok(id)
    }

    #[zbus(name = "CloseNotification")]
    async fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        let removed = self.active_notifications.lock().await.remove(&id).is_some();
        if removed {
            let _ = self.emit_notification_closed(id, 3).await;
        }
        Ok(())
    }

    #[zbus(name = "GetCapabilities")]
    async fn get_capabilities(&self) -> zbus::fdo::Result<Vec<String>> {
        Ok(vec![
            "body".into(),
            "actions".into(),
            "persistence".into(),
            "icon-static".into(),
            "body-markup".into(),
        ])
    }

    #[zbus(name = "GetServerInformation")]
    async fn get_server_information(&self) -> zbus::fdo::Result<(String, String, String, String)> {
        Ok((
            env!("CARGO_PKG_NAME").into(),
            "Kaneru Project".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        ))
    }
}

pub async fn run_server_task(srv: Arc<NotificationServer>) -> Result<(), zbus::Error> {
    let conn = Connection::session().await?;
    conn.request_name("org.freedesktop.Notifications").await?;
    srv.set_connection(conn.clone()).await;
    conn.object_server()
        .at("/org/freedesktop/Notifications", srv.as_ref().clone())
        .await?;
    pending::<()>().await;
    Ok(())
}
