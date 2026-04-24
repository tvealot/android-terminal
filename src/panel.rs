use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelId {
    Logcat,
    Monitor,
    Gradle,
    Processes,
    Issues,
    Files,
    Network,
    Devices,
    Shell,
    AppControl,
    AppData,
    Intents,
    Fps,
    Perf,
}

#[allow(dead_code)]
impl PanelId {
    pub fn slug(self) -> &'static str {
        match self {
            PanelId::Logcat => "logcat",
            PanelId::Monitor => "monitor",
            PanelId::Gradle => "gradle",
            PanelId::Processes => "processes",
            PanelId::Issues => "issues",
            PanelId::Files => "files",
            PanelId::Network => "network",
            PanelId::Devices => "devices",
            PanelId::Shell => "shell",
            PanelId::AppControl => "app",
            PanelId::AppData => "data",
            PanelId::Intents => "intents",
            PanelId::Fps => "fps",
            PanelId::Perf => "perf",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        PANELS.iter().find(|p| p.name == s).map(|p| p.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    None,
    Jvm,
}

pub struct PanelDef {
    pub id: PanelId,
    pub name: &'static str,
    pub toggle_key: char,
    pub focus_key: char,
    pub requires: Feature,
}

pub const PANELS: &[PanelDef] = &[
    PanelDef { id: PanelId::Logcat,     name: "logcat",    toggle_key: '1', focus_key: 'l', requires: Feature::None },
    PanelDef { id: PanelId::Monitor,    name: "monitor",   toggle_key: '2', focus_key: 'm', requires: Feature::None },
    PanelDef { id: PanelId::Gradle,     name: "gradle",    toggle_key: '3', focus_key: 'g', requires: Feature::Jvm  },
    PanelDef { id: PanelId::Processes,  name: "processes", toggle_key: '4', focus_key: 'p', requires: Feature::None },
    PanelDef { id: PanelId::Issues,     name: "issues",    toggle_key: '5', focus_key: 'i', requires: Feature::None },
    PanelDef { id: PanelId::Files,      name: "files",     toggle_key: '6', focus_key: 'f', requires: Feature::None },
    PanelDef { id: PanelId::Network,    name: "network",   toggle_key: '7', focus_key: 'n', requires: Feature::None },
    PanelDef { id: PanelId::Devices,    name: "devices",   toggle_key: '8', focus_key: 'v', requires: Feature::None },
    PanelDef { id: PanelId::Shell,      name: "shell",     toggle_key: '9', focus_key: 's', requires: Feature::None },
    PanelDef { id: PanelId::AppControl, name: "app",       toggle_key: 'A', focus_key: 'a', requires: Feature::None },
    PanelDef { id: PanelId::AppData,    name: "data",      toggle_key: 'B', focus_key: 'b', requires: Feature::None },
    PanelDef { id: PanelId::Intents,    name: "intents",   toggle_key: 'U', focus_key: 'u', requires: Feature::None },
    PanelDef { id: PanelId::Fps,        name: "fps",       toggle_key: 'F', focus_key: 'F', requires: Feature::None },
    PanelDef { id: PanelId::Perf,       name: "perf",      toggle_key: 'H', focus_key: 'H', requires: Feature::None },
];

pub fn by_toggle_key(c: char) -> Option<PanelId> {
    PANELS.iter().find(|p| p.toggle_key == c).map(|p| p.id)
}

pub fn by_focus_key(c: char) -> Option<PanelId> {
    PANELS.iter().find(|p| p.focus_key == c).map(|p| p.id)
}

pub fn def(id: PanelId) -> &'static PanelDef {
    PANELS.iter().find(|p| p.id == id).expect("panel def missing")
}
