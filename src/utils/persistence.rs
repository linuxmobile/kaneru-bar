use crate::utils::notification_impl::Notification;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, fs, io, path::PathBuf};

const NOTIFICATIONS_FILENAME: &str = "notifications.json";

#[derive(Debug)]
pub enum PersistenceError {
    Io(io::Error),
    Json(serde_json::Error),
    DirectoryError(String),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistenceError::Io(e) => write!(f, "I/O error: {}", e),
            PersistenceError::Json(e) => write!(f, "JSON error: {}", e),
            PersistenceError::DirectoryError(s) => write!(f, "Directory error: {}", s),
        }
    }
}

impl Error for PersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            PersistenceError::Io(e) => Some(e),
            PersistenceError::Json(e) => Some(e),
            PersistenceError::DirectoryError(_) => None,
        }
    }
}

impl From<io::Error> for PersistenceError {
    fn from(err: io::Error) -> Self {
        PersistenceError::Io(err)
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(err: serde_json::Error) -> Self {
        PersistenceError::Json(err)
    }
}

fn get_notifications_path() -> Result<PathBuf, PersistenceError> {
    let mut path = dirs::cache_dir().ok_or_else(|| {
        PersistenceError::DirectoryError("Could not determine cache directory".into())
    })?;
    path.push(env!("CARGO_PKG_NAME"));
    fs::create_dir_all(&path)?;
    path.push(NOTIFICATIONS_FILENAME);
    Ok(path)
}

pub fn save_notifications(notifications: &[Notification]) -> Result<(), PersistenceError> {
    let path = get_notifications_path()?;
    let json_data = serde_json::to_string_pretty(notifications)?;
    fs::write(path, json_data)?;
    Ok(())
}

pub fn load_notifications() -> Result<Vec<Notification>, PersistenceError> {
    let path = get_notifications_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let json_data = fs::read_to_string(path)?;
    if json_data.trim().is_empty() {
        return Ok(Vec::new());
    }
    let notifications = serde_json::from_str(&json_data)?;
    Ok(notifications)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum UrgencySerde {
    Low = 0,
    Normal = 1,
    Critical = 2,
}

impl From<crate::utils::notification_impl::Urgency> for UrgencySerde {
    fn from(urgency: crate::utils::notification_impl::Urgency) -> Self {
        match urgency {
            crate::utils::notification_impl::Urgency::Low => UrgencySerde::Low,
            crate::utils::notification_impl::Urgency::Normal => UrgencySerde::Normal,
            crate::utils::notification_impl::Urgency::Critical => UrgencySerde::Critical,
        }
    }
}

impl From<UrgencySerde> for crate::utils::notification_impl::Urgency {
    fn from(urgency: UrgencySerde) -> Self {
        match urgency {
            UrgencySerde::Low => crate::utils::notification_impl::Urgency::Low,
            UrgencySerde::Normal => crate::utils::notification_impl::Urgency::Normal,
            UrgencySerde::Critical => crate::utils::notification_impl::Urgency::Critical,
        }
    }
}
