use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SCALE_STEPS: [f64; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default = "default_scale")]
    pub ui_scale: f64,
    /// Remembered window sizes; `None` falls back to the built-in defaults.
    #[serde(default)]
    pub limits_width: Option<i32>,
    #[serde(default)]
    pub limits_height: Option<i32>,
    #[serde(default)]
    pub dashboard_width: Option<i32>,
    #[serde(default)]
    pub dashboard_height: Option<i32>,
}

fn default_scale() -> f64 {
    1.25
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ui_scale: default_scale(),
            limits_width: None,
            limits_height: None,
            dashboard_width: None,
            dashboard_height: None,
        }
    }
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::creds::home().join(".config"))
        .join("tokenmaxxing")
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load() -> Config {
    let mut config: Config = std::fs::read_to_string(config_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    config.ui_scale = config.ui_scale.clamp(1.0, 2.0);
    config
}

pub fn save(config: &Config) {
    let _ = std::fs::create_dir_all(config_dir());
    if let Ok(json) = serde_json::to_vec_pretty(config) {
        let _ = std::fs::write(config_path(), json);
    }
}

/// Index of the closest preset scale, for wiring the dropdown selection.
pub fn scale_index(scale: f64) -> u32 {
    SCALE_STEPS
        .iter()
        .position(|s| (s - scale).abs() < 0.01)
        .unwrap_or(1) as u32
}
