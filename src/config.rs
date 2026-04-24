use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout::LayoutGrid;
use crate::panel::{PanelId, PANELS};

pub const SCREEN_COUNT: usize = 4;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub gradle: GradleConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub android: AndroidConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GradleConfig {
    pub project_dir: Option<PathBuf>,
    pub default_task: Option<String>,
    pub jar_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AndroidConfig {
    pub package: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

fn default_theme() -> String {
    "dark".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub visible: Vec<PanelId>,
    pub focus: PanelId,
    #[serde(default)]
    pub layout: Option<LayoutGrid>,
    #[serde(default)]
    pub screens: Vec<ScreenState>,
    #[serde(default)]
    pub active_screen: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenState {
    pub visible: Vec<PanelId>,
    pub focus: PanelId,
    #[serde(default)]
    pub layout: Option<LayoutGrid>,
}

impl Default for State {
    fn default() -> Self {
        let screen = ScreenState::default();
        Self {
            visible: screen.visible.clone(),
            focus: screen.focus,
            layout: None,
            screens: vec![screen; SCREEN_COUNT],
            active_screen: 0,
        }
    }
}

impl Default for ScreenState {
    fn default() -> Self {
        Self {
            visible: PANELS
                .iter()
                .filter(|p| p.id != PanelId::Fps)
                .map(|p| p.id)
                .collect(),
            focus: PANELS[0].id,
            layout: None,
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

pub fn update_project_dir(project_dir: &Path) -> std::io::Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let cfg_path = dir.join("config.toml");
    let mut doc: toml::Table = match fs::read_to_string(&cfg_path) {
        Ok(text) => text.parse().unwrap_or_default(),
        Err(_) => toml::Table::new(),
    };
    let gradle = doc
        .entry("gradle".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let toml::Value::Table(g) = gradle {
        g.insert(
            "project_dir".to_string(),
            toml::Value::String(project_dir.display().to_string()),
        );
    }
    let text = toml::to_string_pretty(&doc)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(cfg_path, text)
}

pub fn update_android_package(package: Option<&str>) -> std::io::Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let cfg_path = dir.join("config.toml");
    let mut doc: toml::Table = match fs::read_to_string(&cfg_path) {
        Ok(text) => text.parse().unwrap_or_default(),
        Err(_) => toml::Table::new(),
    };
    let android = doc
        .entry("android".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let toml::Value::Table(a) = android {
        if let Some(package) = package {
            a.insert(
                "package".to_string(),
                toml::Value::String(package.to_string()),
            );
        } else {
            a.remove("package");
        }
    }
    let text = toml::to_string_pretty(&doc)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(cfg_path, text)
}
