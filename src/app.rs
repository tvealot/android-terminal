use std::collections::HashSet;

use crate::config::{save_state, Config, State};
use crate::panel::{def, Feature, PanelId, PANELS};

pub struct App {
    pub config: Config,
    pub visible: HashSet<PanelId>,
    pub focus: PanelId,
    pub jvm_available: bool,
    pub status: Option<StatusFlash>,
    pub show_help: bool,
    pub should_quit: bool,
    pub logcat: crate::logcat::LogcatState,
    pub gradle: crate::gradle::GradleState,
    pub monitor: crate::monitor::MonitorState,
    pub processes: crate::processes::ProcessesState,
    pub input_mode: InputMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    LogcatFilter,
}

pub struct StatusFlash {
    pub text: String,
    pub error: bool,
    pub until: chrono::DateTime<chrono::Local>,
}

impl App {
    pub fn new(config: Config, state: State, jvm_available: bool) -> Self {
        let mut visible: HashSet<PanelId> = state.visible.into_iter().collect();
        if !jvm_available {
            visible.remove(&PanelId::Gradle);
        }
        let focus = if visible.contains(&state.focus) {
            state.focus
        } else {
            visible.iter().copied().next().unwrap_or(PANELS[0].id)
        };

        Self {
            config,
            visible,
            focus,
            jvm_available,
            status: None,
            show_help: false,
            should_quit: false,
            logcat: crate::logcat::LogcatState::default(),
            gradle: crate::gradle::GradleState::default(),
            monitor: crate::monitor::MonitorState::default(),
            processes: crate::processes::ProcessesState::default(),
            input_mode: InputMode::Normal,
        }
    }

    pub fn toggle_panel(&mut self, id: PanelId) {
        let d = def(id);
        if d.requires == Feature::Jvm && !self.jvm_available {
            self.flash(
                "install JDK 17+ to enable Gradle panel".to_string(),
                true,
            );
            return;
        }
        if self.visible.contains(&id) {
            self.visible.remove(&id);
            if self.focus == id {
                self.focus = self.visible.iter().copied().next().unwrap_or(id);
            }
        } else {
            self.visible.insert(id);
            self.focus = id;
        }
        self.persist();
    }

    pub fn focus_panel(&mut self, id: PanelId) {
        if self.visible.contains(&id) {
            self.focus = id;
            self.persist();
        } else {
            self.flash(
                format!("panel '{}' is hidden (Alt+{} to show)", def(id).name, def(id).toggle_key),
                false,
            );
        }
    }

    pub fn flash(&mut self, text: String, error: bool) {
        self.status = Some(StatusFlash {
            text,
            error,
            until: chrono::Local::now() + chrono::Duration::seconds(3),
        });
    }

    pub fn tick_status(&mut self) {
        if let Some(s) = &self.status {
            if chrono::Local::now() > s.until {
                self.status = None;
            }
        }
    }

    pub fn visible_ordered(&self) -> Vec<PanelId> {
        PANELS
            .iter()
            .filter(|p| self.visible.contains(&p.id))
            .map(|p| p.id)
            .collect()
    }

    fn persist(&self) {
        let state = State {
            visible: self.visible_ordered(),
            focus: self.focus,
        };
        let _ = save_state(&state);
    }
}
