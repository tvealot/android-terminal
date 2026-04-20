use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::panel::{PanelId, PANELS};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub gradle: GradleConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GradleConfig {
    pub project_dir: Option<PathBuf>,
    pub default_task: Option<String>,
    pub jar_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self { theme: default_theme() }
    }
}

fn default_theme() -> String {
    "dark".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub visible: Vec<PanelId>,
    pub focus: PanelId,
}

impl Default for State {
    fn default() -> Self {
        Self {
            visible: PANELS.iter().map(|p| p.id).collect(),
            focus: PANELS[0].id,
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("droidscope")
}

pub fn load_config() -> Config {
    let path = config_dir().join("config.toml");
    let Ok(text) = fs::read_to_string(&path) else {
        return Config::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

pub fn load_state() -> State {
    let path = config_dir().join("state.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return State::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_state(state: &State) -> std::io::Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join("state.json");
    let text = serde_json::to_string_pretty(state).unwrap();
    fs::write(path, text)
}
