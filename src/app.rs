use std::collections::HashSet;

use crate::adb::devices::DeviceEntry;
use crate::adb::DeviceHandle;
use crate::config::{save_state, Config, State};
use crate::files::FilesState;
use crate::layout::{LayoutEditor, LayoutGrid};
use crate::panel::{def, Feature, PanelId, PANELS};

pub struct App {
    pub config: Config,
    pub visible: HashSet<PanelId>,
    pub focus: PanelId,
    pub jvm_available: bool,
    pub adb_available: bool,
    pub status: Option<StatusFlash>,
    pub show_help: bool,
    pub should_quit: bool,
    pub logcat: crate::logcat::LogcatState,
    pub gradle: crate::gradle::GradleState,
    pub monitor: crate::monitor::MonitorState,
    pub processes: crate::processes::ProcessesState,
    pub issues: crate::issues::IssuesState,
    pub files: FilesState,
    pub shell: crate::shell::ShellState,
    pub input_mode: InputMode,
    pub device: DeviceHandle,
    pub devices: Vec<DeviceEntry>,
    pub devices_selected: usize,
    pub device_selector: Option<usize>,
    pub package_input: String,
    pub pending_g: bool,
    pub layout: Option<LayoutGrid>,
    pub layout_editor: Option<LayoutEditor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    LogcatFilter,
    LogcatPackage,
    LayoutEdit,
}

pub struct StatusFlash {
    pub text: String,
    pub error: bool,
    pub until: chrono::DateTime<chrono::Local>,
}

impl App {
    pub fn new(
        config: Config,
        state: State,
        jvm_available: bool,
        adb_available: bool,
        device: DeviceHandle,
    ) -> Self {
        let mut visible: HashSet<PanelId> = state.visible.into_iter().collect();
        if !jvm_available {
            visible.remove(&PanelId::Gradle);
        }
        let layout = state.layout.and_then(|g| {
            if g.cells.is_empty() {
                None
            } else {
                Some(g)
            }
        });
        let focus_candidates: Vec<PanelId> = if let Some(g) = &layout {
            g.visible_panels()
        } else {
            visible.iter().copied().collect()
        };
        let focus = if focus_candidates.contains(&state.focus) {
            state.focus
        } else {
            focus_candidates
                .into_iter()
                .next()
                .unwrap_or(PANELS[0].id)
        };

        let files = FilesState::new(config.gradle.project_dir.clone());

        Self {
            config,
            visible,
            focus,
            jvm_available,
            adb_available,
            status: None,
            show_help: false,
            should_quit: false,
            logcat: crate::logcat::LogcatState::default(),
            gradle: crate::gradle::GradleState::default(),
            monitor: crate::monitor::MonitorState::default(),
            processes: crate::processes::ProcessesState::default(),
            issues: crate::issues::IssuesState::default(),
            files,
            shell: crate::shell::ShellState::default(),
            input_mode: InputMode::Normal,
            device,
            devices: Vec::new(),
            devices_selected: 0,
            device_selector: None,
            package_input: String::new(),
            pending_g: false,
            layout,
            layout_editor: None,
        }
    }

    pub fn set_device(&mut self, serial: Option<String>) {
        if let Ok(mut guard) = self.device.lock() {
            *guard = serial.clone();
        }
        match serial {
            Some(s) => self.flash(format!("device: {}", s), false),
            None => self.flash("device: (default)".to_string(), false),
        }
    }

    pub fn current_device(&self) -> Option<String> {
        self.device.lock().ok().and_then(|g| g.clone())
    }

    pub fn cycle_focus(&mut self, forward: bool) {
        let visible = self.visible_ordered();
        if visible.len() < 2 {
            return;
        }
        let Some(pos) = visible.iter().position(|id| *id == self.focus) else {
            self.focus = visible[0];
            return;
        };
        let next = if forward {
            (pos + 1) % visible.len()
        } else {
            (pos + visible.len() - 1) % visible.len()
        };
        self.focus = visible[next];
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
                format!("panel '{}' is hidden ({} to show)", def(id).name, def(id).toggle_key),
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
        if let Some(g) = &self.layout {
            return g.visible_panels();
        }
        PANELS
            .iter()
            .filter(|p| self.visible.contains(&p.id))
            .map(|p| p.id)
            .collect()
    }

    pub fn open_layout_editor(&mut self) {
        let grid = self.layout.clone().unwrap_or_default();
        self.layout_editor = Some(LayoutEditor::new(grid));
        self.input_mode = InputMode::LayoutEdit;
    }

    pub fn close_layout_editor(&mut self, save: bool) {
        let editor = match self.layout_editor.take() {
            Some(e) => e,
            None => {
                self.input_mode = InputMode::Normal;
                return;
            }
        };
        if save {
            if editor.grid.cells.is_empty() {
                self.layout = None;
            } else {
                let grid = editor.grid;
                for p in grid.visible_panels() {
                    self.visible.insert(p);
                }
                if !grid.visible_panels().contains(&self.focus) {
                    if let Some(first) = grid.visible_panels().into_iter().next() {
                        self.focus = first;
                    }
                }
                self.layout = Some(grid);
            }
            self.persist();
            self.flash("layout saved".to_string(), false);
        }
        self.input_mode = InputMode::Normal;
    }

    fn persist(&self) {
        let state = State {
            visible: if self.layout.is_some() {
                PANELS
                    .iter()
                    .filter(|p| self.visible.contains(&p.id))
                    .map(|p| p.id)
                    .collect()
            } else {
                self.visible_ordered()
            },
            focus: self.focus,
            layout: self.layout.clone(),
        };
        let _ = save_state(&state);
    }
}
