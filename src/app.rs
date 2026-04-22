use std::collections::HashSet;

use crate::config::{save_state, Config, State};
use crate::files::FilesState;
use crate::panel::{def, Feature, PanelId, PANELS};

pub struct App {
    pub config: Config,
    pub visible: HashSet<PanelId>,
    pub focus: PanelId,
    pub adb_available: bool,
    pub jvm_available: bool,
    pub devices: Vec<String>,
    pub status: Option<StatusFlash>,
    pub show_help: bool,
    pub should_quit: bool,
    pub logcat: crate::logcat::LogcatState,
    pub gradle: crate::gradle::GradleState,
    pub files: FilesState,
}

pub struct StatusFlash {
    pub text: String,
    pub error: bool,
    pub until: chrono::DateTime<chrono::Local>,
}

impl App {
    pub fn new(config: Config, state: State, adb_available: bool, jvm_available: bool) -> Self {
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
            files: FilesState::new(config.gradle.project_dir.clone()),
            config,
            visible,
            focus,
            adb_available,
            jvm_available,
            devices: Vec::new(),
            status: None,
            show_help: false,
            should_quit: false,
            logcat: crate::logcat::LogcatState::default(),
            gradle: crate::gradle::GradleState::default(),
        }
    }

    pub fn toggle_panel(&mut self, id: PanelId) {
        let d = def(id);
        if d.requires == Feature::Jvm && !self.jvm_available {
            self.flash("install JDK 17+ to enable Gradle panel".to_string(), true);
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
                format!(
                    "panel '{}' is hidden (Alt+{} to show)",
                    def(id).name,
                    def(id).toggle_key
                ),
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
