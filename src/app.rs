use std::collections::HashSet;
use std::path::PathBuf;

use crate::adb::devices::DeviceEntry;
use crate::adb::DeviceHandle;
use crate::config::{
    save_state, save_workspaces, update_android_package, update_default_task, update_project_dir,
    workspace_id, workspace_name, Config, ScreenState, State, WorkspaceLogcat, WorkspaceProfile,
    WorkspaceStore, SCREEN_COUNT,
};
use crate::emulator_picker::EmulatorPicker;
use crate::files::FilesState;
use crate::fps::{self, FpsState};
use crate::layout::{LayoutEditor, LayoutGrid};
use crate::panel::{def, Feature, PanelId, PANELS};
use crate::perf::{self, PerfState};
use crate::project_picker::ProjectPicker;

pub struct App {
    pub config: Config,
    pub visible: HashSet<PanelId>,
    pub focus: PanelId,
    pub jvm_available: bool,
    pub adb_available: bool,
    pub status: Option<StatusFlash>,
    pub show_help: bool,
    pub should_quit: bool,
    pub target_package: Option<String>,
    pub logcat: crate::logcat::LogcatState,
    pub gradle: crate::gradle::GradleState,
    pub monitor: crate::monitor::MonitorState,
    pub processes: crate::processes::ProcessesState,
    pub issues: crate::issues::IssuesState,
    pub files: FilesState,
    pub shell: crate::shell::ShellState,
    pub fps: FpsState,
    pub perf: PerfState,
    pub app_control: crate::app_control::AppControlState,
    pub device_actions: crate::device_actions::DeviceActionsState,
    pub app_data: crate::app_data::AppDataState,
    pub manifest: crate::manifest::ManifestState,
    pub intents: crate::intents::IntentsState,
    pub input_mode: InputMode,
    pub device: DeviceHandle,
    pub devices: Vec<DeviceEntry>,
    pub devices_selected: usize,
    pub device_selector: Option<usize>,
    pub package_input: String,
    pub fps_package_input: String,
    pub perf_package_input: String,
    pub target_package_input: String,
    pub deep_link_input: String,
    pub pending_g: bool,
    pub layout: Option<LayoutGrid>,
    pub screens: Vec<ScreenState>,
    pub active_screen: usize,
    pub layout_editor: Option<LayoutEditor>,
    pub project_picker: Option<ProjectPicker>,
    pub workspaces: WorkspaceStore,
    pub workspace_picker: Option<WorkspacePicker>,
    pub emulator_picker: Option<EmulatorPicker>,
    pub zoom: Option<PanelId>,
}

pub struct WorkspacePicker {
    pub selected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    LogcatFilter,
    LogcatPackage,
    FpsPackage,
    PerfPackage,
    TargetPackage,
    DeepLinkUrl,
    DeviceText,
    DeviceTap,
    DeviceLocale,
    DeviceFontScale,
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
        workspaces: WorkspaceStore,
        jvm_available: bool,
        adb_available: bool,
        device: DeviceHandle,
        fps_package: fps::FpsPackageHandle,
        perf_package: perf::PerfPackageHandle,
    ) -> Self {
        let active_workspace = workspaces.active_workspace().cloned();
        let mut config = config;
        let mut state = state;
        if let Some(workspace) = &active_workspace {
            config.gradle.project_dir = Some(workspace.project_dir.clone());
            config.gradle.default_task = workspace.default_task.clone();
            config.android.package = workspace.package.clone();
            if !workspace.screens.is_empty() {
                state.screens = workspace.screens.clone();
                state.active_screen = workspace.active_screen;
            }
            if let Ok(mut guard) = device.lock() {
                *guard = workspace.preferred_device.clone();
            }
        }
        let mut screens = state.screens;
        if screens.is_empty() {
            screens.push(ScreenState {
                visible: state.visible,
                focus: state.focus,
                layout: state.layout,
            });
        }
        while screens.len() < SCREEN_COUNT {
            screens.push(ScreenState::default());
        }
        screens.truncate(SCREEN_COUNT);
        let screens: Vec<ScreenState> = screens
            .into_iter()
            .map(|screen| normalize_screen(screen, jvm_available))
            .collect();
        let active_screen = state.active_screen.min(screens.len().saturating_sub(1));
        let active = screens
            .get(active_screen)
            .cloned()
            .unwrap_or_else(ScreenState::default);
        let visible: HashSet<PanelId> = active.visible.into_iter().collect();
        let focus = active.focus;
        let layout = active.layout;

        let files = FilesState::new(config.gradle.project_dir.clone());
        let target_package = config.android.package.clone();

        let mut app = Self {
            config,
            visible,
            focus,
            jvm_available,
            adb_available,
            status: None,
            show_help: false,
            should_quit: false,
            target_package,
            logcat: crate::logcat::LogcatState::default(),
            gradle: crate::gradle::GradleState::default(),
            monitor: crate::monitor::MonitorState::default(),
            processes: crate::processes::ProcessesState::default(),
            issues: crate::issues::IssuesState::default(),
            files,
            shell: crate::shell::ShellState::default(),
            fps: FpsState::new(fps_package),
            perf: PerfState::new(perf_package),
            app_control: crate::app_control::AppControlState::default(),
            device_actions: crate::device_actions::DeviceActionsState::default(),
            app_data: crate::app_data::AppDataState::default(),
            manifest: crate::manifest::ManifestState::default(),
            intents: crate::intents::IntentsState::default(),
            input_mode: InputMode::Normal,
            device,
            devices: Vec::new(),
            devices_selected: 0,
            device_selector: None,
            package_input: String::new(),
            fps_package_input: String::new(),
            perf_package_input: String::new(),
            target_package_input: String::new(),
            deep_link_input: String::new(),
            pending_g: false,
            layout,
            screens,
            active_screen,
            layout_editor: None,
            project_picker: None,
            workspaces,
            workspace_picker: None,
            emulator_picker: None,
            zoom: None,
        };
        if let Some(workspace) = active_workspace {
            app.apply_workspace_logcat(&workspace.logcat);
        }
        app
    }

    pub fn set_target_package(&mut self, package: Option<String>) {
        self.config.android.package = package.clone();
        self.target_package = package.clone();
        self.app_data.reset_for_package();
        self.manifest.reset_for_package();
        match update_android_package(package.as_deref()) {
            Ok(()) => match package {
                Some(pkg) => self.flash(format!("target package: {}", pkg), false),
                None => self.flash("target package cleared".to_string(), false),
            },
            Err(e) => self.flash(format!("save config: {}", e), true),
        }
    }

    pub fn apply_project_dir(&mut self, path: PathBuf) {
        self.config.gradle.project_dir = Some(path.clone());
        self.files.set_root(Some(path.clone()));
        match update_project_dir(&path) {
            Ok(()) => self.flash(format!("project: {}", path.display()), false),
            Err(e) => self.flash(format!("save config: {}", e), true),
        }
    }

    pub fn save_current_workspace(&mut self) {
        let Some(project_dir) = self.config.gradle.project_dir.clone() else {
            self.flash("pick a project before saving a workspace".to_string(), true);
            return;
        };
        self.save_active_screen_snapshot();
        let workspace = WorkspaceProfile {
            id: workspace_id(&project_dir),
            name: workspace_name(&project_dir),
            project_dir,
            default_task: self.config.gradle.default_task.clone(),
            package: self.target_package.clone(),
            preferred_device: self.current_device(),
            logcat: WorkspaceLogcat {
                filter: self.logcat.filter.clone(),
                min_level: self.logcat.min_level,
                package_filter: self.logcat.filter_package.clone(),
                use_regex: self.logcat.use_regex,
            },
            screens: self.screens.clone(),
            active_screen: self.active_screen,
        };
        let name = workspace.name.clone();
        self.workspaces.upsert(workspace);
        match save_workspaces(&self.workspaces) {
            Ok(()) => self.flash(format!("workspace saved: {}", name), false),
            Err(e) => self.flash(format!("save workspace: {}", e), true),
        }
    }

    pub fn apply_workspace(&mut self, workspace: &WorkspaceProfile) {
        self.config.gradle.project_dir = Some(workspace.project_dir.clone());
        self.config.gradle.default_task = workspace.default_task.clone();
        self.files.set_root(Some(workspace.project_dir.clone()));
        self.set_target_package_without_flash(workspace.package.clone());
        self.apply_workspace_logcat(&workspace.logcat);
        self.logcat.lines.clear();
        if let Ok(mut guard) = self.device.lock() {
            *guard = workspace.preferred_device.clone();
        }

        if !workspace.screens.is_empty() {
            let screens = workspace
                .screens
                .clone()
                .into_iter()
                .map(|screen| normalize_screen(screen, self.jvm_available))
                .collect::<Vec<_>>();
            self.screens = screens;
            while self.screens.len() < SCREEN_COUNT {
                self.screens.push(ScreenState::default());
            }
            self.screens.truncate(SCREEN_COUNT);
            self.active_screen = workspace
                .active_screen
                .min(self.screens.len().saturating_sub(1));
            let screen = self.screens[self.active_screen].clone();
            self.visible = screen.visible.into_iter().collect();
            self.focus = screen.focus;
            self.layout = screen.layout;
            self.zoom = None;
            self.layout_editor = None;
            self.input_mode = InputMode::Normal;
        }

        self.workspaces.active = Some(workspace.id.clone());
        if let Err(e) = save_workspaces(&self.workspaces) {
            self.flash(format!("save active workspace: {}", e), true);
            return;
        }
        let _ = update_project_dir(&workspace.project_dir);
        let _ = update_default_task(workspace.default_task.as_deref());
        let _ = update_android_package(workspace.package.as_deref());
        self.persist();
        self.flash(format!("workspace: {}", workspace.name), false);
    }

    pub fn workspace_for_project(&self, path: &std::path::Path) -> Option<WorkspaceProfile> {
        self.workspaces.find_by_project(path).cloned()
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

    fn set_target_package_without_flash(&mut self, package: Option<String>) {
        self.config.android.package = package.clone();
        self.target_package = package;
        self.app_data.reset_for_package();
        self.manifest.reset_for_package();
    }

    fn apply_workspace_logcat(&mut self, logcat: &WorkspaceLogcat) {
        self.logcat.filter = logcat.filter.clone();
        self.logcat.min_level = logcat.min_level;
        self.logcat.filter_package = logcat.package_filter.clone();
        self.logcat.filter_pid = None;
        self.logcat.use_regex = logcat.use_regex;
        self.logcat.recompile();
        self.logcat.scroll = 0;
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
            if self.zoom == Some(id) {
                self.zoom = None;
            }
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

    pub fn switch_screen(&mut self, index: usize) {
        if index >= self.screens.len() {
            return;
        }
        if index == self.active_screen {
            self.flash(format!("screen {}", index + 1), false);
            return;
        }
        self.save_active_screen_snapshot();
        self.active_screen = index;

        let screen = self.screens[index].clone();
        self.visible = screen.visible.into_iter().collect();
        self.focus = screen.focus;
        self.layout = screen.layout;
        self.zoom = None;
        self.layout_editor = None;
        self.input_mode = InputMode::Normal;
        self.persist();
        self.flash(format!("screen {}", index + 1), false);
    }

    pub fn cycle_screen(&mut self, forward: bool) {
        if self.screens.len() < 2 {
            return;
        }
        let len = self.screens.len();
        let next = if forward {
            (self.active_screen + 1) % len
        } else {
            (self.active_screen + len - 1) % len
        };
        self.switch_screen(next);
    }

    pub fn screen_label(&self) -> String {
        format!("{}/{}", self.active_screen + 1, self.screens.len())
    }

    fn persist(&self) {
        let mut screens = self.screens.clone();
        if self.active_screen < screens.len() {
            screens[self.active_screen] = self.current_screen_snapshot();
        }
        let current = screens
            .get(self.active_screen)
            .cloned()
            .unwrap_or_else(|| self.current_screen_snapshot());
        let state = State {
            visible: current.visible.clone(),
            focus: current.focus,
            layout: current.layout.clone(),
            screens,
            active_screen: self.active_screen,
        };
        let _ = save_state(&state);
    }

    fn save_active_screen_snapshot(&mut self) {
        if self.active_screen < self.screens.len() {
            self.screens[self.active_screen] = self.current_screen_snapshot();
        }
    }

    fn current_screen_snapshot(&self) -> ScreenState {
        ScreenState {
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
        }
    }
}

fn normalize_screen(mut screen: ScreenState, jvm_available: bool) -> ScreenState {
    if !jvm_available {
        screen.visible.retain(|id| *id != PanelId::Gradle);
        if let Some(grid) = screen.layout.as_mut() {
            grid.cells.retain(|cell| cell.panel != PanelId::Gradle);
        }
    }
    if let Some(grid) = screen.layout.as_ref() {
        if grid.cells.is_empty() || grid.cols == 0 || grid.rows == 0 {
            screen.layout = None;
        }
    }
    if let Some(grid) = &screen.layout {
        for panel in grid.visible_panels() {
            screen.visible.push(panel);
        }
    }
    let mut seen = HashSet::new();
    screen.visible.retain(|id| seen.insert(*id));

    let focus_candidates = if let Some(grid) = &screen.layout {
        grid.visible_panels()
    } else {
        screen.visible.clone()
    };
    if !focus_candidates.contains(&screen.focus) {
        screen.focus = focus_candidates.into_iter().next().unwrap_or(PANELS[0].id);
    }
    screen
}
