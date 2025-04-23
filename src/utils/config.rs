use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, fs, io, path::PathBuf};

#[derive(Debug, Deserialize, Serialize)]
pub struct BarConfig {
    pub height: i32,
    #[serde(default)]
    pub distro_icon_override: Option<String>,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            height: 30,
            distro_icon_override: None,
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Yaml(serde_yaml::Error),
    Dirs,
}

impl From<io::Error> for ConfigError {
    fn from(err: io::Error) -> Self {
        ConfigError::Io(err)
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(err: serde_yaml::Error) -> Self {
        ConfigError::Yaml(err)
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "I/O error: {}", e),
            ConfigError::Yaml(e) => write!(f, "YAML parsing error: {}", e),
            ConfigError::Dirs => write!(f, "Could not determine config directory"),
        }
    }
}

impl Error for ConfigError {}

fn get_config_path() -> Result<PathBuf, ConfigError> {
    let mut path = dirs::config_dir().ok_or(ConfigError::Dirs)?;
    path.push("kaneru");
    path.push("config.yaml");
    Ok(path)
}

fn ensure_config_exists() -> Result<(), ConfigError> {
    let config_path = get_config_path()?;

    if !config_path.exists() {
        let config_dir = config_path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Config path has no parent"))?;
        fs::create_dir_all(config_dir)?;

        let default_config = BarConfig::default();
        let yaml_string = serde_yaml::to_string(&default_config)?;
        fs::write(config_path, yaml_string)?;
    }

    Ok(())
}

pub fn load_config() -> BarConfig {
    match load_config_internal() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load or create config: {}. Using default.", e);
            BarConfig::default()
        }
    }
}

fn load_config_internal() -> Result<BarConfig, ConfigError> {
    ensure_config_exists()?;
    let config_path = get_config_path()?;
    let contents = fs::read_to_string(config_path)?;
    let config: BarConfig = serde_yaml::from_str(&contents)?;
    Ok(config)
}
