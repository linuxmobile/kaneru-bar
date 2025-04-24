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

    pub async fn set_connection(&self, connection: Connection) {
        let mut conn_guard = self.connection.lock().await;
        *conn_guard = Some(connection);
    }

    pub async fn emit_notification_closed(&self, id: u32, reason: u32) -> zbus::Result<()> {
        if let Some(conn) = &*self.connection.lock().await {
            conn.emit_signal(
                None::<()>,
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "NotificationClosed",
                &(id, reason),
            )
            .await
        } else {
            Err(zbus::Error::Failure("No D-Bus connection available".into()))
        }
    }

    pub async fn emit_action_invoked(&self, id: u32, action_key: &str) -> zbus::Result<()> {
        if let Some(conn) = &*self.connection.lock().await {
            conn.emit_signal(
                None::<()>,
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "ActionInvoked",
                &(id, action_key),
            )
            .await
        } else {
            Err(zbus::Error::Failure("No D-Bus connection available".into()))
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
        let new_id = if replaces_id == 0 {
            NEXT_NOTIFICATION_ID.fetch_add(1, Ordering::Relaxed)
        } else {
            replaces_id
        };

        let mut owned_hints_temp = HashMap::new();
        for (k, v) in hints_raw {
            match v.try_to_owned() {
                Ok(owned_v) => {
                    owned_hints_temp.insert(k, owned_v);
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to convert hint '{}' to owned value: {}",
                        k, e
                    );
                }
            }
        }

        let urgency = owned_hints_temp
            .get("urgency")
            .and_then(|v| Urgency::try_from(v.clone()).ok())
            .unwrap_or(Urgency::Normal);

        let image_path = owned_hints_temp
            .get("image-path")
            .and_then(|v| String::try_from(v.clone()).ok());

        let resident = owned_hints_temp
            .get("resident")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false);

        let notification = Notification::new(
            new_id,
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
            let mut active_notifications = self.active_notifications.lock().await;
            active_notifications.insert(new_id, notification.clone());
        }

        if let Err(e) = self.notify_tx.send(notification).await {
            eprintln!("Failed to send notification to UI manager: {}", e);
            return Err(zbus::fdo::Error::Failed(format!(
                "Internal error: Failed to queue notification for display: {}",
                e
            )));
        }

        Ok(new_id)
    }

    #[zbus(name = "CloseNotification")]
    async fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        let reason = 3;

        let notification_exists = {
            let mut active_notifications = self.active_notifications.lock().await;
            active_notifications.remove(&id).is_some()
        };

        if notification_exists {
            if let Err(e) = self.emit_notification_closed(id, reason).await {
                eprintln!("Failed to emit NotificationClosed signal: {}", e);
            }
            Ok(())
        } else {
            eprintln!(
                "Request to close non-existent notification via D-Bus: id={}",
                id
            );
            Ok(())
        }
    }

    #[zbus(name = "GetCapabilities")]
    async fn get_capabilities(&self) -> zbus::fdo::Result<Vec<String>> {
        Ok(vec![
            "body".to_string(),
            "actions".to_string(),
            "persistence".to_string(),
            "icon-static".to_string(),
            "body-markup".to_string(),
        ])
    }

    #[zbus(name = "GetServerInformation")]
    async fn get_server_information(&self) -> zbus::fdo::Result<(String, String, String, String)> {
        Ok((
            env!("CARGO_PKG_NAME").to_string(),
            "Kaneru Project".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.2".to_string(),
        ))
    }
}

pub async fn run_server_task(server: Arc<NotificationServer>) -> Result<(), zbus::Error> {
    println!("Starting notification D-Bus server task...");

    let connection = Connection::session().await?;
    connection
        .request_name("org.freedesktop.Notifications")
        .await?;

    server.set_connection(connection.clone()).await;

    connection
        .object_server()
        .at("/org/freedesktop/Notifications", server.as_ref().clone())
        .await?;

    println!("Notification server registered on D-Bus.");

    pending::<()>().await;

    Ok(())
}

pub mod signals {
    use super::*;

    pub async fn emit_notification_closed(
        server: &Arc<NotificationServer>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()> {
        server.emit_notification_closed(id, reason).await
    }

    pub async fn emit_action_invoked(
        server: &Arc<NotificationServer>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()> {
        server.emit_action_invoked(id, action_key).await
    }
}
