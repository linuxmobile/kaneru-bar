use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleType {
    AppMenu,
    ActiveClient,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct BarConfig {
    pub font: Option<String>,
    pub modules_left: Vec<ModuleType>,
    pub modules_center: Vec<ModuleType>,
    pub modules_right: Vec<ModuleType>,
    pub distro_icon_override: Option<String>,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            font: Some("Sans 10".to_string()),
            modules_left: vec![ModuleType::AppMenu, ModuleType::ActiveClient],
            modules_center: vec![],
            modules_right: vec![],
            distro_icon_override: None,
        }
    }
}

fn get_config_path() -> Result<PathBuf> {
    let mut path = dirs::config_dir().context("Could not determine config directory")?;
    path.push("kaneru");
    path.push("config.yaml");
    Ok(path)
}

fn ensure_config_exists() -> Result<()> {
    let config_path = get_config_path()?;

    if !config_path.exists() {
        println!(
            "Config file not found at {:?}. Creating default config.",
            config_path
        );
        let config_dir = config_path
            .parent()
            .context("Config path has no parent directory")?;
        fs::create_dir_all(config_dir)
            .with_context(|| format!("Failed to create config directory {:?}", config_dir))?;

        let default_config = BarConfig::default();
        let yaml_string = serde_yaml::to_string(&default_config)
            .context("Failed to serialize default config to YAML")?;
        fs::write(&config_path, yaml_string)
            .with_context(|| format!("Failed to write default config to {:?}", config_path))?;
        println!("Default config written successfully.");
    }

    Ok(())
}

pub fn load_config() -> BarConfig {
    match load_config_internal() {
        Ok(config) => {
            println!("Successfully loaded config: {:?}", config);
            config
        }
        Err(e) => {
            eprintln!("Failed to load or create config: {:?}. Using default.", e);
            BarConfig::default()
        }
    }
}

fn load_config_internal() -> Result<BarConfig> {
    ensure_config_exists()?;
    let config_path = get_config_path()?;
    println!("Loading config from: {:?}", config_path);
    let contents = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
    let config: BarConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML from config file: {:?}", config_path))?;
    Ok(config)
}
