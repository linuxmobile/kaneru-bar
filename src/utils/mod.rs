pub(crate) mod config;
mod distro;
pub mod niri;
pub(crate) mod notification;
pub mod notification_manager;
pub mod notification_server;
mod persistence;
mod style;

pub use config::{load_config, BarConfig, ModuleType, NotificationPosition};
pub use distro::get_distro_icon_name;
pub use notification::{Notification, Urgency};
pub use persistence::{load_notifications, save_notifications};
pub use style::apply_css;

pub(crate) use notification as notification_impl;
