mod config;
mod distro;
pub mod niri;
mod style;

pub use config::{load_config, BarConfig, ModuleType};
pub use distro::get_distro_icon_name;
pub use style::apply_css;
