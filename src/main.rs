mod adb;
mod app;
mod app_control;
mod app_control_ui;
mod app_data;
mod app_data_ui;
mod clipboard;
mod command_palette;
mod config;
mod device_actions;
mod device_actions_ui;
mod devices_ui;
mod dispatch;
mod emulator_picker;
mod files;
mod files_ui;
mod fps;
mod fps_ui;
mod gradle;
mod gradle_ui;
mod intents;
mod intents_ui;
mod issues;
mod issues_ui;
mod keymap;
mod layout;
mod logcat;
mod logcat_ui;
mod manifest;
mod manifest_ui;
mod monitor;
mod monitor_ui;
mod network_ui;
mod panel;
mod perf;
mod perf_ui;
mod processes;
mod processes_ui;
mod project_picker;
mod shell;
mod shell_ui;
mod theme;
mod ui;

use std::io::{self, Stdout};
use std::process::Child;
use std::time::Duration;

use color_eyre::eyre::{Result, WrapErr};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyEvent,
    KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect, Size};
use ratatui::Terminal;

use crate::app::{App, InputMode};
use crate::dispatch::{DispatchContext, Event};
use crate::panel::{by_focus_key, by_toggle_key, PanelId, PANELS};

struct Runtime {
    logcat_child: Option<Child>,
}

impl Runtime {
    fn restart_logcat(
        &mut self,
        app: &App,
        dispatcher: &DispatchContext,
    ) {
        if let Some(mut child) = self.logcat_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        match adb::logcat::spawn(&app.device, dispatcher.tx.clone()) {
            Ok(child) => self.logcat_child = Some(child),
            Err(e) => {
                let _ = dispatcher.tx.send(Event::Status {
                    text: format!("logcat spawn failed: {}", e),
                    error: true,
                });
            }
        }
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let cfg = config::load_config();
    let state = config::load_state();
    let workspaces = config::load_workspaces();
    let jvm_available = gradle::jvm_available();
    let adb_available = adb::is_available();
    let device = adb::new_handle();
    let fps_package = fps::new_package_handle();
    let perf_package = perf::new_package_handle();
    let app = App::new(
        cfg,
        state,
        workspaces,
        jvm_available,
        adb_available,
        device.clone(),
        fps_package.clone(),
        perf_package.clone(),
    );

    let dispatcher = DispatchContext::new();
    let mut runtime = Runtime { logcat_child: None };

    if adb_available {
        runtime.restart_logcat(&app, &dispatcher);
        monitor::spawn_poller(device.clone(), dispatcher.tx.clone());
        processes::spawn_poller(device.clone(), dispatcher.tx.clone());
        adb::devices::spawn_poller(dispatcher.tx.clone());
        fps::spawn_poller(device.clone(), fps_package, dispatcher.tx.clone());
        perf::spawn_poller(device.clone(), perf_package, dispatcher.tx.clone());
    } else {
        let _ = dispatcher.tx.send(Event::Status {
            text: "adb not found in PATH — logcat/monitor disabled".to_string(),
            error: true,
        });
    }
    gradle::spawn_host_poller(dispatcher.tx.clone());

    let mut terminal = setup_terminal()?;
    let result = run_loop(&mut terminal, app, dispatcher, runtime);
    result
        .and(restore_terminal(&mut terminal))
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().wrap_err("enable_raw_mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).wrap_err("enter alt screen")?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).wrap_err("terminal init")
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), DisableMouseCapture).ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    Ok(())
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut app: App,
    dispatcher: DispatchContext,
    mut runtime: Runtime,
) -> Result<()> {
    let mut mouse_capture = false;
    loop {
        for ev in dispatcher.drain() {
            match ev {
                Event::Logcat(line) => {
                    app.issues.detect(&line);
                    app.logcat.push(line);
                }
                Event::Gradle(ev) => {
                    if let gradle::GradleEvent::Variants { items, .. } = &ev {
                        if let Some(picker) = app.variant_picker.as_mut() {
                            picker.variants = items.clone();
                            picker.loading = false;
                            picker.selected = pick_initial_variant(
                                &picker.variants,
                                app.config.gradle.default_task.as_deref(),
                            );
                        }
                    }
                    app.gradle.apply(ev);
                }
                Event::HostGradle(list) => {
                    app.gradle.host_procs = list;
                    app.gradle.clamp_selected();
                }
                Event::Monitor(sample) => app.monitor.push(sample),
                Event::Processes(procs) => app.processes.replace(procs),
                Event::Devices(list) => {
                    app.devices = list;
                    if app.devices.is_empty() {
                        app.devices_selected = 0;
                    } else if app.devices_selected >= app.devices.len() {
                        app.devices_selected = app.devices.len() - 1;
                    }
                }
                Event::Projects(list) => {
                    if let Some(picker) = app.project_picker.as_mut() {
                        picker.entries = list;
                        picker.loading = false;
                        picker.selected = 0;
                        if let Some(cur) = app.config.gradle.project_dir.as_ref() {
                            if let Some(i) = picker.entries.iter().position(|e| &e.path == cur) {
                                picker.selected = i;
                            }
                        }
                    }
                }
                Event::Emulators(list) => {
                    if let Some(picker) = app.emulator_picker.as_mut() {
                        picker.entries = list;
                        picker.loading = false;
                        picker.selected = 0;
                    }
                }
                Event::Fps(sample) => app.fps.push(sample),
                Event::Perf(sample) => app.perf.push(sample),
                Event::AppControl(result) => {
                    app.app_control.running = false;
                    app.app_control.pending_confirm = None;
                    let error = !result.success;
                    let text = result.summary.clone();
                    app.app_control.last = Some(result);
                    app.flash(text, error);
                }
                Event::DeviceAction(result) => {
                    app.device_actions.running = false;
                    let error = !result.success;
                    let text = result.summary.clone();
                    app.device_actions.last = Some(result);
                    app.flash(text, error);
                }
                Event::AppData(event) => {
                    if app_data_event_matches_target(&app, &event) {
                        let status = app_data_status(&event);
                        app.app_data.apply(event);
                        if let Some((text, error)) = status {
                            app.flash(text, error);
                        }
                    }
                }
                Event::Manifest(report) => {
                    app.manifest.running = false;
                    app.manifest.scroll = 0;
                    let error = !report.success;
                    let text = report.summary.clone();
                    app.manifest.last = Some(report);
                    app.flash(text, error);
                }
                Event::Intent(result) => {
                    app.intents.running = false;
                    if result.success {
                        app.intents.remember(&result.url);
                    }
                    let error = !result.success;
                    let text = result.summary.clone();
                    app.intents.last = Some(result);
                    app.flash(text, error);
                }
                Event::Status { text, error } => app.flash(text, error),
            }
        }
        app.tick_status();
        if let Some(msg) = app.shell.poll_exit() {
            app.flash(msg, true);
        }
        let term_size = terminal.size()?;
        update_shell_size(&mut app, term_size.height, term_size.width);
        ensure_shell_started(&mut app);

        let theme = theme::by_name(&app.config.ui.theme);
        terminal.draw(|f| ui::render(f, &app, theme))?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                CEvent::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        handle_key(&mut app, key, &dispatcher, &mut runtime);
                    }
                }
                CEvent::Mouse(mouse) if app.mouse_enabled => {
                    handle_mouse(&mut app, mouse, term_size);
                }
                _ => {}
            }
        }
        if app.mouse_enabled != mouse_capture {
            let command_result = if app.mouse_enabled {
                execute!(terminal.backend_mut(), EnableMouseCapture)
            } else {
                execute!(terminal.backend_mut(), DisableMouseCapture)
            };
            match command_result {
                Ok(()) => mouse_capture = app.mouse_enabled,
                Err(e) => {
                    app.mouse_enabled = mouse_capture;
                    app.flash(format!("mouse mode failed: {}", e), true);
                }
            }
        }

        if app.should_quit {
            if let Some(mut child) = runtime.logcat_child.take() {
                let _ = child.kill();
            }
            if mouse_capture {
                execute!(terminal.backend_mut(), DisableMouseCapture).ok();
            }
            app.shell.stop();
            return Ok(());
        }
    }
}

fn handle_key(
    app: &mut App,
    key: KeyEvent,
    dispatcher: &DispatchContext,
    runtime: &mut Runtime,
) {
    // Input-mode keys take priority.
    match app.input_mode {
        InputMode::LogcatFilter => {
            return handle_filter_key(app, key);
        }
        InputMode::LogcatPackage => {
            return handle_package_key(app, key, dispatcher);
        }
        InputMode::FpsPackage => {
            return handle_fps_package_key(app, key);
        }
        InputMode::PerfPackage => {
            return handle_perf_package_key(app, key);
        }
        InputMode::TargetPackage => {
            return handle_target_package_key(app, key);
        }
        InputMode::DeepLinkUrl => {
            return handle_deep_link_url_key(app, key);
        }
        InputMode::DeviceText
        | InputMode::DeviceTap
        | InputMode::DeviceLocale
        | InputMode::DeviceFontScale => {
            return handle_device_action_input_key(app, key, dispatcher);
        }
        InputMode::LayoutEdit => {
            return handle_layout_editor_key(app, key);
        }
        InputMode::Normal => {}
    }

    let raw = key;
    let key = keymap::normalize(key);

    // Command palette overlay: consumes keys while open.
    if app.command_palette.is_some() {
        return handle_command_palette_key(app, key, dispatcher, runtime);
    }

    // Ctrl+P opens command palette globally (works even from shell focus).
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
    {
        open_command_palette(app);
        return;
    }

    if key.modifiers.contains(KeyModifiers::ALT)
        && matches!(key.code, KeyCode::Char('m') | KeyCode::Char('M'))
    {
        toggle_mouse_mode(app);
        return;
    }

    // Workspace picker overlay: consumes keys while open.
    if app.workspace_picker.is_some() {
        return handle_workspace_picker_key(app, key, dispatcher, runtime);
    }

    // Variant picker overlay: consumes keys while open.
    if app.variant_picker.is_some() {
        return handle_variant_picker_key(app, key);
    }

    // Project picker overlay: consumes keys while open.
    if app.project_picker.is_some() {
        return handle_project_picker_key(app, key, dispatcher, runtime);
    }

    // Emulator picker overlay: consumes keys while open.
    if app.emulator_picker.is_some() {
        return handle_emulator_picker_key(app, key);
    }

    // Device selector overlay: consumes keys while open.
    if let Some(idx) = app.device_selector {
        return handle_device_selector(app, key, idx, dispatcher, runtime);
    }

    if key.modifiers.contains(KeyModifiers::ALT) {
        if let KeyCode::Char(c) = key.code {
            if let Some(n) = c.to_digit(10) {
                let index = n as usize;
                if (1..=app.screens.len()).contains(&index) {
                    app.switch_screen(index - 1);
                    return;
                }
            }
        }
    }

    match key.code {
        KeyCode::Char('[') => {
            app.cycle_screen(false);
            return;
        }
        KeyCode::Char(']') => {
            app.cycle_screen(true);
            return;
        }
        _ => {}
    }

    if let KeyCode::Char(c) = key.code {
        if c == '0' {
            app.open_layout_editor();
            return;
        }
        if let Some(id) = by_toggle_key(c) {
            app.toggle_panel(id);
            return;
        }
    }

    // Files panel owns most keys while focused; Tab still cycles focus
    // unless the detail pane is open (where it toggles tree ↔ detail).
    if app.focus == PanelId::Files {
        let tab_to_global = matches!(key.code, KeyCode::Tab) && !app.files.detail_open;
        if !tab_to_global && app.files.handle_key(key) {
            return;
        }
    }

    if app.focus == PanelId::AppControl && handle_app_control_key(app, key, dispatcher) {
        return;
    }

    if app.focus == PanelId::DeviceActions && handle_device_actions_key(app, key, dispatcher) {
        return;
    }

    if app.focus == PanelId::AppData && handle_app_data_key(app, key, dispatcher) {
        return;
    }

    if app.focus == PanelId::Manifest && handle_manifest_key(app, key, dispatcher) {
        return;
    }

    if app.focus == PanelId::Intents && handle_intents_key(app, key, dispatcher) {
        return;
    }

    // Shell panel captures all keys while focused. Escape hatch: Ctrl+\.
    if app.focus == PanelId::Shell && app.shell.active {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('\\')) {
            app.cycle_focus(true);
            return;
        }
        if let Some(bytes) = shell_key_to_bytes(raw) {
            app.shell.write(&bytes);
        }
        return;
    }

    // Any key other than `g` resets a pending `gg` sequence.
    if !matches!(key.code, KeyCode::Char('g')) {
        app.pending_g = false;
    }

    match key.code {
        KeyCode::Esc if app.show_help => {
            app.show_help = false;
        }
        KeyCode::Esc if app.zoom.is_some() => {
            app.zoom = None;
        }
        KeyCode::Esc if app.focus == PanelId::Issues && app.issues.expanded.is_some() => {
            app.issues.close_detail();
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
        }
        KeyCode::Char('z') => {
            toggle_zoom(app);
        }
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
        }
        KeyCode::Char('r') => {
            start_gradle(app, dispatcher);
        }
        KeyCode::Char('d') => {
            open_device_selector(app);
        }
        KeyCode::Char('w') => {
            open_project_picker(app, dispatcher);
        }
        KeyCode::Char('W') => {
            open_workspace_picker(app);
        }
        KeyCode::Char('S') => {
            app.save_current_workspace();
        }
        KeyCode::Char('V') => {
            open_variant_picker(app, dispatcher);
        }
        KeyCode::Char('e') => {
            open_emulator_picker(app, dispatcher);
        }
        KeyCode::Char('F') => {
            app.toggle_panel(PanelId::Fps);
        }
        KeyCode::Char('P') if app.focus == PanelId::Fps => {
            app.fps_package_input = app.fps.current_package().unwrap_or_default();
            app.input_mode = InputMode::FpsPackage;
        }
        KeyCode::Char('X') if app.focus == PanelId::Fps => {
            app.fps.set_package(None);
            app.flash("fps package cleared".to_string(), false);
        }
        KeyCode::Char('P') if app.focus == PanelId::Perf => {
            app.perf_package_input = app.perf.current_package().unwrap_or_default();
            app.input_mode = InputMode::PerfPackage;
        }
        KeyCode::Char('X') if app.focus == PanelId::Perf => {
            app.perf.set_package(None);
            app.flash("perf package cleared".to_string(), false);
        }
        KeyCode::Char('/') if app.focus == PanelId::Logcat => {
            app.input_mode = InputMode::LogcatFilter;
        }
        KeyCode::Char('L') if app.focus == PanelId::Logcat => {
            app.logcat.min_level = app.logcat.min_level.next_cycle();
            app.flash(
                format!("logcat level: {}+", app.logcat.min_level.short()),
                false,
            );
        }
        KeyCode::Char('P') if app.focus == PanelId::Logcat => {
            app.package_input = app
                .logcat
                .filter_package
                .clone()
                .unwrap_or_default();
            app.input_mode = InputMode::LogcatPackage;
        }
        KeyCode::Char('X') if app.focus == PanelId::Logcat => {
            app.logcat.clear_package_filter();
            app.flash("logcat package filter cleared".to_string(), false);
        }
        KeyCode::Char(' ') if app.focus == PanelId::Logcat => {
            app.logcat.paused = !app.logcat.paused;
            let msg = if app.logcat.paused { "logcat paused" } else { "logcat resumed" };
            app.flash(msg.to_string(), false);
        }
        KeyCode::Char('C') if app.focus == PanelId::Logcat => {
            app.logcat.clear();
            app.flash("logcat cleared".to_string(), false);
        }
        KeyCode::Char('R') if app.focus == PanelId::Logcat => {
            app.logcat.toggle_regex();
            let msg = if app.logcat.use_regex { "logcat: regex on" } else { "logcat: regex off" };
            app.flash(msg.to_string(), false);
        }
        KeyCode::Char('j') | KeyCode::Down if app.focus == PanelId::Logcat => {
            app.logcat.scroll_down(1);
            app.pending_g = false;
        }
        KeyCode::Char('k') | KeyCode::Up if app.focus == PanelId::Logcat => {
            app.logcat.scroll_up(1);
            app.pending_g = false;
        }
        KeyCode::PageDown if app.focus == PanelId::Logcat => {
            app.logcat.scroll_down(20);
        }
        KeyCode::PageUp if app.focus == PanelId::Logcat => {
            app.logcat.scroll_up(20);
        }
        KeyCode::Char('G') if app.focus == PanelId::Logcat => {
            app.logcat.scroll_to_bottom();
            app.pending_g = false;
        }
        KeyCode::Char('g') if app.focus == PanelId::Logcat => {
            if app.pending_g {
                app.logcat.scroll_to_top();
                app.pending_g = false;
            } else {
                app.pending_g = true;
            }
        }
        KeyCode::Char('C') if app.focus == PanelId::Issues => {
            app.issues.clear();
            app.flash("issues cleared".to_string(), false);
        }
        KeyCode::Char('y') if app.focus == PanelId::Issues => {
            copy_selected_stacktrace(app);
        }
        KeyCode::Tab => {
            app.cycle_focus(true);
        }
        KeyCode::BackTab => {
            app.cycle_focus(false);
        }
        KeyCode::Char('j') | KeyCode::Down if app.focus == PanelId::Processes => {
            if !app.processes.processes.is_empty() {
                app.processes.selected =
                    (app.processes.selected + 1).min(app.processes.processes.len() - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up if app.focus == PanelId::Processes => {
            app.processes.selected = app.processes.selected.saturating_sub(1);
        }
        KeyCode::Char('j') | KeyCode::Down if app.focus == PanelId::Issues => {
            app.issues.move_down();
        }
        KeyCode::Char('k') | KeyCode::Up if app.focus == PanelId::Issues => {
            app.issues.move_up();
        }
        KeyCode::Enter if app.focus == PanelId::Issues => {
            app.issues.toggle_expand();
        }
        KeyCode::Char('j') | KeyCode::Down if app.focus == PanelId::Gradle => {
            if !app.gradle.host_procs.is_empty() {
                app.gradle.selected =
                    (app.gradle.selected + 1).min(app.gradle.host_procs.len() - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up if app.focus == PanelId::Gradle => {
            app.gradle.selected = app.gradle.selected.saturating_sub(1);
        }
        KeyCode::Char('K') if app.focus == PanelId::Gradle => {
            if let Some(pid) = app.gradle.selected_pid() {
                match gradle::kill_host(pid) {
                    Ok(()) => app.flash(format!("sent SIGTERM to pid {}", pid), false),
                    Err(e) => app.flash(format!("kill {} failed: {}", pid, e), true),
                }
            } else {
                app.flash("no process selected".to_string(), true);
            }
        }
        KeyCode::Char('j') | KeyCode::Down if app.focus == PanelId::Devices => {
            if !app.devices.is_empty() {
                app.devices_selected =
                    (app.devices_selected + 1).min(app.devices.len() - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up if app.focus == PanelId::Devices => {
            app.devices_selected = app.devices_selected.saturating_sub(1);
        }
        KeyCode::Enter if app.focus == PanelId::Devices => {
            switch_to_selected_device(app, dispatcher, runtime);
        }
        KeyCode::Char(c) => {
            if let Some(id) = by_focus_key(c) {
                app.focus_panel(id);
            }
        }
        _ => {}
    }
}

fn handle_mouse(
    app: &mut App,
    mouse: MouseEvent,
    term_size: Size,
) {
    let Some((panel, area)) = panel_at(app, term_size, mouse.column, mouse.row) else {
        return;
    };
    app.focus = panel;
    match mouse.kind {
        MouseEventKind::ScrollUp => mouse_scroll(app, panel, true),
        MouseEventKind::ScrollDown => mouse_scroll(app, panel, false),
        MouseEventKind::Down(MouseButton::Left) => {
            mouse_left_click(app, panel, area, mouse.column, mouse.row);
        }
        _ => {}
    }
}

fn mouse_scroll(app: &mut App, panel: PanelId, up: bool) {
    match panel {
        PanelId::Logcat => {
            if up {
                app.logcat.scroll_up(3);
            } else {
                app.logcat.scroll_down(3);
            }
            app.pending_g = false;
        }
        PanelId::Issues => {
            if app.issues.expanded.is_some() {
                if up {
                    app.issues.detail_scroll = app.issues.detail_scroll.saturating_sub(3);
                } else {
                    app.issues.detail_scroll = app.issues.detail_scroll.saturating_add(3);
                }
            } else if up {
                app.issues.move_up();
            } else {
                app.issues.move_down();
            }
        }
        PanelId::Processes => select_delta(
            &mut app.processes.selected,
            app.processes.processes.len(),
            up,
        ),
        PanelId::Gradle => select_delta(&mut app.gradle.selected, app.gradle.host_procs.len(), up),
        PanelId::Devices => select_delta(&mut app.devices_selected, app.devices.len(), up),
        PanelId::Files => {
            if app.files.detail_open && app.files.detail_focused {
                if up {
                    app.files.detail_scroll = app.files.detail_scroll.saturating_sub(3);
                } else {
                    app.files.detail_scroll = app.files.detail_scroll.saturating_add(3);
                }
            } else {
                let len = app.files.flatten_visible().len();
                select_delta(&mut app.files.selected_index, len, up);
            }
        }
        PanelId::AppControl => {
            if up {
                app.app_control.move_up();
            } else {
                app.app_control.move_down();
            }
        }
        PanelId::DeviceActions => {
            if up {
                app.device_actions.move_up();
            } else {
                app.device_actions.move_down();
            }
        }
        PanelId::AppData => {
            if app.app_data.preview.is_some() && app.app_data.preview_focused {
                if up {
                    app.app_data.preview_scroll = app.app_data.preview_scroll.saturating_sub(3);
                } else {
                    app.app_data.preview_scroll = app.app_data.preview_scroll.saturating_add(3);
                }
            } else if up {
                app.app_data.move_up();
            } else {
                app.app_data.move_down();
            }
        }
        PanelId::Manifest => {
            if up {
                app.manifest.scroll_up(3);
            } else {
                app.manifest.scroll_down(3);
            }
        }
        _ => {}
    }
}

fn mouse_left_click(
    app: &mut App,
    panel: PanelId,
    area: Rect,
    x: u16,
    y: u16,
) {
    let inner = inset(area);
    match panel {
        PanelId::Processes => {
            if let Some(row) = local_row(inner, x, y).and_then(|r| r.checked_sub(1)) {
                select_row(&mut app.processes.selected, app.processes.processes.len(), row);
            }
        }
        PanelId::Gradle => {
            if let Some(row) = local_row(inner, x, y) {
                let active = app.gradle.active.len() as u16;
                if let Some(proc_row) = row.checked_sub(active) {
                    select_row(&mut app.gradle.selected, app.gradle.host_procs.len(), proc_row);
                }
            }
        }
        PanelId::Issues => {
            if app.issues.expanded.is_some() {
                app.issues.toggle_expand();
                return;
            }
            if let Some(row) = local_row(inner, x, y) {
                let height = inner.height.saturating_sub(1) as usize;
                let offset = app.issues.selected.saturating_sub(height.saturating_sub(1));
                let index = offset + row as usize;
                if index < app.issues.issues.len() {
                    app.issues.selected = index;
                }
            }
        }
        PanelId::Devices => {
            if let Some(row) = local_row(inner, x, y).and_then(|r| r.checked_sub(1)) {
                select_row(&mut app.devices_selected, app.devices.len(), row);
            }
        }
        PanelId::Files => click_files(app, inner, x, y),
        PanelId::AppControl => click_app_control(app, inner, x, y),
        PanelId::DeviceActions => click_device_actions(app, inner, x, y),
        PanelId::AppData => click_app_data(app, inner, x, y),
        PanelId::Manifest => app.manifest.scroll = y.saturating_sub(inner.y) as usize,
        _ => {}
    }
}

fn click_files(app: &mut App, inner: Rect, x: u16, y: u16) {
    let tree = if app.files.detail_open {
        let cols = split_cols(inner, 42, 58);
        if contains(cols[1], x, y) {
            app.files.detail_focused = true;
            return;
        }
        app.files.detail_focused = false;
        cols[0]
    } else {
        inner
    };
    if !contains(tree, x, y) {
        return;
    }
    let list_y = tree.y + if app.files.detail_open { 2 } else { 1 };
    if y < list_y {
        return;
    }
    let flat = app.files.flatten_visible();
    let visible_height = tree.height.saturating_sub(if app.files.detail_open { 2 } else { 1 });
    let selected = app.files.selected_index.min(flat.len().saturating_sub(1));
    let start = if selected >= visible_height as usize {
        selected - visible_height as usize + 1
    } else {
        0
    };
    select_row_from_start(&mut app.files.selected_index, flat.len(), start, y - list_y);
}

fn click_app_control(app: &mut App, inner: Rect, x: u16, y: u16) {
    let cols = split_cols(inner, 42, 58);
    let Some(row) = local_row(cols[0], x, y).and_then(|r| r.checked_sub(2)) else {
        return;
    };
    let stride = if cols[0].height > 8 { 2 } else { 1 };
    select_row(
        &mut app.app_control.selected,
        crate::app_control::ACTIONS.len(),
        row / stride,
    );
}

fn click_device_actions(app: &mut App, inner: Rect, x: u16, y: u16) {
    let cols = split_cols(inner, 44, 56);
    let Some(row) = local_row(cols[0], x, y).and_then(|r| r.checked_sub(2)) else {
        return;
    };
    let available = cols[0].height.saturating_sub(2) as usize;
    let selected = app.device_actions.selected;
    let start = if selected >= available {
        selected + 1 - available
    } else {
        0
    };
    select_row_from_start(
        &mut app.device_actions.selected,
        crate::device_actions::ACTIONS.len(),
        start,
        row,
    );
}

fn click_app_data(app: &mut App, inner: Rect, x: u16, y: u16) {
    let list = if app.app_data.preview.is_some() {
        let cols = split_cols(inner, 44, 56);
        if contains(cols[1], x, y) {
            app.app_data.preview_focused = true;
            return;
        }
        app.app_data.preview_focused = false;
        cols[0]
    } else {
        inner
    };
    let Some(row) = local_row(list, x, y).and_then(|r| r.checked_sub(2)) else {
        return;
    };
    let visible_height = list.height.saturating_sub(2) as usize;
    let selected = app
        .app_data
        .selected
        .min(app.app_data.entries.len().saturating_sub(1));
    let start = if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    };
    select_row_from_start(&mut app.app_data.selected, app.app_data.entries.len(), start, row);
}

fn select_delta(selected: &mut usize, len: usize, up: bool) {
    if len == 0 {
        *selected = 0;
    } else if up {
        *selected = selected.saturating_sub(1);
    } else {
        *selected = (*selected + 1).min(len - 1);
    }
}

fn select_row(selected: &mut usize, len: usize, row: u16) {
    if len > 0 {
        *selected = (row as usize).min(len - 1);
    }
}

fn select_row_from_start(selected: &mut usize, len: usize, start: usize, row: u16) {
    if len > 0 {
        let index = start + row as usize;
        if index < len {
            *selected = index;
        }
    }
}

fn panel_at(
    app: &App,
    term_size: Size,
    x: u16,
    y: u16,
) -> Option<(PanelId, Rect)> {
    let full = Rect {
        x: 0,
        y: 0,
        width: term_size.width,
        height: term_size.height,
    };
    let body = Rect {
        x: full.x,
        y: full.y + 1,
        width: full.width,
        height: full.height.saturating_sub(2),
    };
    if let Some(id) = app.zoom {
        return contains(body, x, y).then_some((id, body));
    }
    if let Some(grid) = &app.layout {
        for cell in &grid.cells {
            let area = crate::layout::cell_rect(body, grid, cell.x, cell.y, cell.w, cell.h);
            if contains(area, x, y) {
                return Some((cell.panel, area));
            }
        }
        return None;
    }
    let visible = app.visible_ordered();
    if visible.is_empty() {
        return None;
    }
    let constraints: Vec<Constraint> = visible
        .iter()
        .map(|_| Constraint::Ratio(1, visible.len() as u32))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(body);
    for (i, id) in visible.iter().enumerate() {
        let mut area = rows[i];
        if *id == PanelId::Monitor {
            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Ratio(2, 3), Constraint::Ratio(1, 3)])
                .split(area);
            area = split[1];
        }
        if contains(area, x, y) {
            return Some((*id, area));
        }
    }
    None
}

fn split_cols(area: Rect, left_percent: u16, right_percent: u16) -> [Rect; 2] {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_percent),
            Constraint::Percentage(right_percent),
        ])
        .split(area);
    [cols[0], cols[1]]
}

fn local_row(area: Rect, x: u16, y: u16) -> Option<u16> {
    contains(area, x, y).then_some(y - area.y)
}

fn inset(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

fn handle_layout_editor_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.close_layout_editor(false);
            return;
        }
        KeyCode::Enter => {
            app.close_layout_editor(true);
            return;
        }
        _ => {}
    }
    let Some(editor) = app.layout_editor.as_mut() else {
        app.input_mode = InputMode::Normal;
        return;
    };
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => editor.move_cursor(-1, 0),
        KeyCode::Char('l') | KeyCode::Right => editor.move_cursor(1, 0),
        KeyCode::Char('k') | KeyCode::Up => editor.move_cursor(0, -1),
        KeyCode::Char('j') | KeyCode::Down => editor.move_cursor(0, 1),
        KeyCode::Char('v') | KeyCode::Char(' ') => editor.toggle_selection(),
        KeyCode::Char('x') | KeyCode::Char('d') => editor.delete_at_cursor(),
        KeyCode::Char('c') => editor.clear(),
        KeyCode::Char('[') => editor.resize_cols(-1),
        KeyCode::Char(']') => editor.resize_cols(1),
        KeyCode::Char('-') => editor.resize_rows(-1),
        KeyCode::Char('=') | KeyCode::Char('+') => editor.resize_rows(1),
        KeyCode::Char(c) => {
            if let Some(panel) = PANELS.iter().find(|p| p.toggle_key == c) {
                editor.assign(panel.id);
            }
        }
        _ => {}
    }
}

fn handle_filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
        }
        KeyCode::Backspace => {
            app.logcat.filter.pop();
            app.logcat.recompile();
        }
        KeyCode::Char(c) => {
            app.logcat.filter.push(c);
            app.logcat.recompile();
        }
        _ => {}
    }
}

fn handle_package_key(app: &mut App, key: KeyEvent, dispatcher: &DispatchContext) {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.package_input.clear();
        }
        KeyCode::Enter => {
            let pkg = app.package_input.trim().to_string();
            app.input_mode = InputMode::Normal;
            app.package_input.clear();
            if pkg.is_empty() {
                app.logcat.clear_package_filter();
                return;
            }
            apply_package_filter(app, &pkg, dispatcher);
        }
        KeyCode::Backspace => {
            app.package_input.pop();
        }
        KeyCode::Char(c) => {
            app.package_input.push(c);
        }
        _ => {}
    }
}

fn apply_package_filter(app: &mut App, pkg: &str, dispatcher: &DispatchContext) {
    match query_pid(&app.device, pkg) {
        Ok(pid) => {
            app.logcat.filter_package = Some(pkg.to_string());
            app.logcat.filter_pid = Some(pid);
            let _ = dispatcher.tx.send(Event::Status {
                text: format!("logcat: filtering {} (pid {})", pkg, pid),
                error: false,
            });
        }
        Err(e) => {
            let _ = dispatcher.tx.send(Event::Status {
                text: format!("package {}: {}", pkg, e),
                error: true,
            });
        }
    }
}

fn query_pid(handle: &adb::DeviceHandle, package: &str) -> Result<u32, String> {
    let output = adb::command(handle)
        .args(["shell", "pidof", "-s", package])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("process not running".to_string());
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    raw.split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "process not running".to_string())
}

fn shell_key_to_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    use KeyCode::*;
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let mut out = Vec::new();
    if alt {
        out.push(0x1b);
    }
    match key.code {
        Char(c) => {
            if ctrl {
                let b: u8 = match c {
                    'a'..='z' => (c as u8) - b'a' + 1,
                    'A'..='Z' => (c as u8) - b'A' + 1,
                    '@' | ' ' => 0,
                    '[' => 0x1b,
                    '\\' => 0x1c,
                    ']' => 0x1d,
                    '^' => 0x1e,
                    '_' => 0x1f,
                    '?' => 0x7f,
                    _ => (c as u8) & 0x1f,
                };
                out.push(b);
            } else {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
        Enter => out.push(b'\r'),
        Backspace => out.push(0x7f),
        Tab => {
            if shift {
                out.extend_from_slice(b"\x1b[Z");
            } else {
                out.push(b'\t');
            }
        }
        BackTab => out.extend_from_slice(b"\x1b[Z"),
        Esc => out.push(0x1b),
        Up => out.extend_from_slice(b"\x1b[A"),
        Down => out.extend_from_slice(b"\x1b[B"),
        Right => out.extend_from_slice(b"\x1b[C"),
        Left => out.extend_from_slice(b"\x1b[D"),
        Home => out.extend_from_slice(b"\x1b[H"),
        End => out.extend_from_slice(b"\x1b[F"),
        PageUp => out.extend_from_slice(b"\x1b[5~"),
        PageDown => out.extend_from_slice(b"\x1b[6~"),
        Delete => out.extend_from_slice(b"\x1b[3~"),
        Insert => out.extend_from_slice(b"\x1b[2~"),
        _ => return None,
    }
    Some(out)
}

fn ensure_shell_started(app: &mut App) {
    if app.focus != PanelId::Shell {
        return;
    }
    if app.shell.active {
        return;
    }
    let serial = app.current_device();
    match app.shell.start(serial.as_deref()) {
        Ok(()) => app.flash("shell started".to_string(), false),
        Err(e) => app.flash(format!("shell: {}", e), true),
    }
}

fn update_shell_size(app: &mut App, term_rows: u16, term_cols: u16) {
    if !app.visible.contains(&PanelId::Shell) {
        return;
    }
    let visible = app.visible_ordered();
    let count = visible.len() as u16;
    if count == 0 {
        return;
    }
    // header=1, footer=1 → body height
    let body = term_rows.saturating_sub(2);
    let panel_h = body / count;
    let inner_h = panel_h.saturating_sub(2);
    let inner_w = term_cols.saturating_sub(2);
    app.shell.resize(inner_h, inner_w);
}

fn switch_to_selected_device(
    app: &mut App,
    dispatcher: &DispatchContext,
    runtime: &mut Runtime,
) {
    let Some(entry) = app.devices.get(app.devices_selected).cloned() else {
        app.flash("no devices connected".to_string(), true);
        return;
    };
    if !entry.is_ready() {
        app.flash(
            format!("device {} is {}, skipping", entry.serial, entry.state),
            true,
        );
        return;
    }
    app.set_device(Some(entry.serial));
    app.logcat.lines.clear();
    app.logcat.clear_package_filter();
    runtime.restart_logcat(app, dispatcher);
}

fn open_device_selector(app: &mut App) {
    if app.devices.is_empty() {
        app.flash("no devices connected".to_string(), true);
        return;
    }
    let current = app.current_device();
    let idx = app
        .devices
        .iter()
        .position(|d| Some(&d.serial) == current.as_ref())
        .unwrap_or(0);
    app.device_selector = Some(idx);
}

fn handle_device_selector(
    app: &mut App,
    key: KeyEvent,
    idx: usize,
    dispatcher: &DispatchContext,
    runtime: &mut Runtime,
) {
    let len = app.devices.len();
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.device_selector = None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if len > 0 {
                app.device_selector = Some((idx + 1).min(len - 1));
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.device_selector = Some(idx.saturating_sub(1));
        }
        KeyCode::Enter => {
            if let Some(entry) = app.devices.get(idx).cloned() {
                if !entry.is_ready() {
                    app.flash(
                        format!("device {} is {}, skipping", entry.serial, entry.state),
                        true,
                    );
                } else {
                    app.set_device(Some(entry.serial));
                    app.logcat.lines.clear();
                    app.logcat.clear_package_filter();
                    runtime.restart_logcat(app, dispatcher);
                }
            }
            app.device_selector = None;
        }
        _ => {}
    }
}

fn open_project_picker(app: &mut App, dispatcher: &DispatchContext) {
    let root = project_picker::default_root();
    app.project_picker = Some(project_picker::ProjectPicker::new(root.clone()));
    app.flash(format!("scanning {} for Android projects…", root.display()), false);
    project_picker::spawn_scan(root, dispatcher.tx.clone());
}

fn handle_project_picker_key(
    app: &mut App,
    key: KeyEvent,
    dispatcher: &DispatchContext,
    runtime: &mut Runtime,
) {
    let Some(picker) = app.project_picker.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.project_picker = None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if !picker.entries.is_empty() {
                picker.selected = (picker.selected + 1).min(picker.entries.len() - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            picker.selected = picker.selected.saturating_sub(1);
        }
        KeyCode::Enter => {
            let path = picker.entries.get(picker.selected).map(|e| e.path.clone());
            app.project_picker = None;
            if let Some(path) = path {
                if let Some(workspace) = app.workspace_for_project(&path) {
                    apply_workspace(app, workspace, dispatcher, runtime);
                } else {
                    app.apply_project_dir(path);
                }
            }
        }
        _ => {}
    }
}

fn open_workspace_picker(app: &mut App) {
    if app.workspaces.workspaces.is_empty() {
        app.flash(
            "no saved workspaces; press S to save current project".to_string(),
            true,
        );
        return;
    }
    let selected = app
        .workspaces
        .active
        .as_ref()
        .and_then(|active| {
            app.workspaces
                .workspaces
                .iter()
                .position(|w| &w.id == active)
        })
        .unwrap_or(0);
    app.workspace_picker = Some(crate::app::WorkspacePicker { selected });
}

fn handle_workspace_picker_key(
    app: &mut App,
    key: KeyEvent,
    dispatcher: &DispatchContext,
    runtime: &mut Runtime,
) {
    let Some(picker) = app.workspace_picker.as_mut() else {
        return;
    };
    let len = app.workspaces.workspaces.len();
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.workspace_picker = None;
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            app.workspace_picker = None;
            app.save_current_workspace();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if len > 0 {
                picker.selected = (picker.selected + 1).min(len - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            picker.selected = picker.selected.saturating_sub(1);
        }
        KeyCode::Enter => {
            let workspace = app.workspaces.workspaces.get(picker.selected).cloned();
            app.workspace_picker = None;
            if let Some(workspace) = workspace {
                apply_workspace(app, workspace, dispatcher, runtime);
            }
        }
        _ => {}
    }
}

fn apply_workspace(
    app: &mut App,
    workspace: config::WorkspaceProfile,
    dispatcher: &DispatchContext,
    runtime: &mut Runtime,
) {
    let package_filter = workspace.logcat.package_filter.clone();
    app.apply_workspace(&workspace);
    if let Some(pkg) = package_filter {
        if !pkg.trim().is_empty() {
            apply_package_filter(app, &pkg, dispatcher);
        }
    }
    runtime.restart_logcat(app, dispatcher);
}

fn open_variant_picker(app: &mut App, dispatcher: &DispatchContext) {
    if !app.jvm_available {
        app.flash("JDK not found; cannot list variants".to_string(), true);
        return;
    }
    let Some(project) = app.config.gradle.project_dir.clone() else {
        app.flash(
            "set [gradle].project_dir before picking a variant".to_string(),
            true,
        );
        return;
    };
    let jar = app
        .config
        .gradle
        .jar_path
        .clone()
        .unwrap_or_else(gradle::default_jar_path);
    if !jar.exists() {
        app.flash(
            format!("gradle-agent.jar not found at {}", jar.display()),
            true,
        );
        return;
    }
    let initial_mode = app
        .config
        .gradle
        .default_task
        .as_deref()
        .and_then(gradle::task_to_variant)
        .map(|(prefix, _)| match prefix {
            "install" => crate::app::VariantMode::Install,
            _ => crate::app::VariantMode::Assemble,
        })
        .unwrap_or(crate::app::VariantMode::Assemble);
    app.variant_picker = Some(crate::app::VariantPicker::new(initial_mode));
    match gradle::spawn_list_variants(&jar, &project, dispatcher.tx.clone()) {
        Ok(()) => app.flash("scanning Gradle variants…".to_string(), false),
        Err(e) => {
            app.variant_picker = None;
            app.flash(format!("variant scan failed: {}", e), true);
        }
    }
}

fn pick_initial_variant(variants: &[String], current_task: Option<&str>) -> usize {
    let Some(task) = current_task else {
        return 0;
    };
    let Some((_, current_variant)) = gradle::task_to_variant(task) else {
        return 0;
    };
    variants
        .iter()
        .position(|v| v == &current_variant)
        .unwrap_or(0)
}

fn handle_variant_picker_key(app: &mut App, key: KeyEvent) {
    let Some(picker) = app.variant_picker.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.variant_picker = None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if !picker.variants.is_empty() {
                picker.selected = (picker.selected + 1).min(picker.variants.len() - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            picker.selected = picker.selected.saturating_sub(1);
        }
        KeyCode::Char('t') | KeyCode::Tab => {
            picker.mode = picker.mode.toggle();
        }
        KeyCode::Char('a') => {
            picker.mode = crate::app::VariantMode::Assemble;
        }
        KeyCode::Char('i') => {
            picker.mode = crate::app::VariantMode::Install;
        }
        KeyCode::Enter => {
            let mode = picker.mode;
            let variant = picker.variants.get(picker.selected).cloned();
            app.variant_picker = None;
            if let Some(variant) = variant {
                app.apply_variant(&variant, mode);
            }
        }
        _ => {}
    }
}

fn open_emulator_picker(app: &mut App, dispatcher: &DispatchContext) {
    app.emulator_picker = Some(emulator_picker::EmulatorPicker::new());
    app.flash("scanning emulator AVDs…".to_string(), false);
    emulator_picker::spawn_scan(dispatcher.tx.clone());
}

fn handle_emulator_picker_key(app: &mut App, key: KeyEvent) {
    let Some(picker) = app.emulator_picker.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.emulator_picker = None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if !picker.entries.is_empty() {
                picker.selected = (picker.selected + 1).min(picker.entries.len() - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            picker.selected = picker.selected.saturating_sub(1);
        }
        KeyCode::Enter => {
            let avd = picker.entries.get(picker.selected).cloned();
            app.emulator_picker = None;
            if let Some(name) = avd {
                match emulator_picker::launch(&name) {
                    Ok(()) => app.flash(format!("launching AVD: {}", name), false),
                    Err(e) => app.flash(format!("emulator: {}", e), true),
                }
            }
        }
        _ => {}
    }
}

fn open_command_palette(app: &mut App) {
    let commands = command_palette::build_commands(app.jvm_available);
    app.command_palette = Some(command_palette::CommandPalette::new(commands));
}

fn handle_command_palette_key(
    app: &mut App,
    key: KeyEvent,
    dispatcher: &DispatchContext,
    runtime: &mut Runtime,
) {
    let Some(palette) = app.command_palette.as_mut() else {
        return;
    };
    let len = palette.filtered().len();
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Esc => {
            app.command_palette = None;
        }
        KeyCode::Enter => {
            let kind = palette.current_kind();
            app.command_palette = None;
            if let Some(kind) = kind {
                execute_palette_command(app, kind, dispatcher, runtime);
            }
        }
        KeyCode::Down => palette.move_down(len),
        KeyCode::Up => palette.move_up(),
        KeyCode::Char('n') if ctrl => palette.move_down(len),
        KeyCode::Char('j') if ctrl => palette.move_down(len),
        KeyCode::Char('p') if ctrl => palette.move_up(),
        KeyCode::Char('k') if ctrl => palette.move_up(),
        KeyCode::Backspace => {
            palette.query.pop();
            palette.selected = 0;
        }
        KeyCode::Char(c) if !ctrl && !alt => {
            palette.query.push(c);
            palette.selected = 0;
        }
        _ => {}
    }
}

fn execute_palette_command(
    app: &mut App,
    kind: command_palette::CommandKind,
    dispatcher: &DispatchContext,
    runtime: &mut Runtime,
) {
    use command_palette::CommandKind::*;
    match kind {
        Quit => app.should_quit = true,
        ToggleHelp => app.show_help = !app.show_help,
        ToggleMouse => toggle_mouse_mode(app),
        PickProject => open_project_picker(app, dispatcher),
        OpenWorkspaces => open_workspace_picker(app),
        SaveWorkspace => app.save_current_workspace(),
        RunGradle => start_gradle(app, dispatcher),
        PickVariant => open_variant_picker(app, dispatcher),
        PickDevice => open_device_selector(app),
        LaunchEmulator => open_emulator_picker(app, dispatcher),
        CycleFocusNext => app.cycle_focus(true),
        CycleFocusPrev => app.cycle_focus(false),
        NextScreen => app.cycle_screen(true),
        PrevScreen => app.cycle_screen(false),
        EditLayout => app.open_layout_editor(),
        ToggleZoom => toggle_zoom(app),
        TogglePanel(id) => app.toggle_panel(id),
        FocusPanel(id) => app.focus_panel(id),
    }
    let _ = runtime;
}

fn handle_fps_package_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.fps_package_input.clear();
        }
        KeyCode::Enter => {
            let pkg = app.fps_package_input.trim().to_string();
            app.input_mode = InputMode::Normal;
            app.fps_package_input.clear();
            if pkg.is_empty() {
                app.fps.set_package(None);
                app.flash("fps package cleared".to_string(), false);
            } else {
                app.fps.set_package(Some(pkg.clone()));
                app.flash(format!("fps: tracking {}", pkg), false);
            }
        }
        KeyCode::Backspace => {
            app.fps_package_input.pop();
        }
        KeyCode::Char(c) => {
            app.fps_package_input.push(c);
        }
        _ => {}
    }
}

fn handle_perf_package_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.perf_package_input.clear();
        }
        KeyCode::Enter => {
            let pkg = app.perf_package_input.trim().to_string();
            app.input_mode = InputMode::Normal;
            app.perf_package_input.clear();
            if pkg.is_empty() {
                app.perf.set_package(None);
                app.flash("perf package cleared".to_string(), false);
            } else {
                app.perf.set_package(Some(pkg.clone()));
                app.flash(format!("perf: tracking {}", pkg), false);
            }
        }
        KeyCode::Backspace => {
            app.perf_package_input.pop();
        }
        KeyCode::Char(c) => {
            app.perf_package_input.push(c);
        }
        _ => {}
    }
}

fn handle_target_package_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.target_package_input.clear();
        }
        KeyCode::Enter => {
            let pkg = app.target_package_input.trim().to_string();
            app.input_mode = InputMode::Normal;
            app.target_package_input.clear();
            if pkg.is_empty() {
                app.set_target_package(None);
            } else if is_valid_package(&pkg) {
                app.set_target_package(Some(pkg));
            } else {
                app.flash(
                    "package may contain only letters, digits, dot, underscore".to_string(),
                    true,
                );
            }
        }
        KeyCode::Backspace => {
            app.target_package_input.pop();
        }
        KeyCode::Char(c) => {
            app.target_package_input.push(c);
        }
        _ => {}
    }
}

fn handle_deep_link_url_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.deep_link_input.clear();
        }
        KeyCode::Enter => {
            let url = app.deep_link_input.trim().to_string();
            app.input_mode = InputMode::Normal;
            app.deep_link_input.clear();
            app.intents.set_url(url);
        }
        KeyCode::Backspace => {
            app.deep_link_input.pop();
        }
        KeyCode::Char(c) => {
            app.deep_link_input.push(c);
        }
        _ => {}
    }
}

fn handle_app_control_key(app: &mut App, key: KeyEvent, dispatcher: &DispatchContext) -> bool {
    match key.code {
        KeyCode::Char('P') => {
            app.target_package_input = app.target_package.clone().unwrap_or_default();
            app.input_mode = InputMode::TargetPackage;
            true
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.app_control.move_down();
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.app_control.move_up();
            true
        }
        KeyCode::Enter => {
            start_app_action(app, dispatcher, false);
            true
        }
        KeyCode::Char('!') => {
            start_app_action(app, dispatcher, true);
            true
        }
        _ => false,
    }
}

fn start_app_action(app: &mut App, dispatcher: &DispatchContext, confirm: bool) {
    if app.app_control.running {
        app.flash("app action already running".to_string(), false);
        return;
    }
    let Some(package) = app.target_package.clone() else {
        app.flash("set target package with P".to_string(), true);
        return;
    };
    let action = app.app_control.selected_action();
    if action.destructive() && !confirm {
        app.app_control.pending_confirm = Some(action);
        app.flash(
            format!("press ! to confirm {} for {}", action.label(), package),
            true,
        );
        return;
    }
    if action.destructive() && app.app_control.pending_confirm != Some(action) {
        app.app_control.pending_confirm = Some(action);
        app.flash(
            format!(
                "press ! again to confirm {} for {}",
                action.label(),
                package
            ),
            true,
        );
        return;
    }
    app.app_control.pending_confirm = None;
    app.app_control.running = true;
    app.app_control.last = None;
    crate::app_control::spawn_action(app.device.clone(), package, action, dispatcher.tx.clone());
}

fn handle_device_actions_key(
    app: &mut App,
    key: KeyEvent,
    dispatcher: &DispatchContext,
) -> bool {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.device_actions.move_down();
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.device_actions.move_up();
            true
        }
        KeyCode::Enter => {
            start_device_action(app, dispatcher);
            true
        }
        _ => false,
    }
}

fn start_device_action(app: &mut App, dispatcher: &DispatchContext) {
    if app.device_actions.running {
        app.flash("device action already running".to_string(), false);
        return;
    }
    let action = app.device_actions.selected_action();
    match action {
        device_actions::DeviceAction::InputText => {
            app.device_actions.input.clear();
            app.input_mode = InputMode::DeviceText;
        }
        device_actions::DeviceAction::Tap => {
            app.device_actions.input.clear();
            app.input_mode = InputMode::DeviceTap;
        }
        device_actions::DeviceAction::Locale => {
            app.device_actions.input = "en-US".to_string();
            app.input_mode = InputMode::DeviceLocale;
        }
        device_actions::DeviceAction::FontScale => {
            app.device_actions.input = "1.0".to_string();
            app.input_mode = InputMode::DeviceFontScale;
        }
        _ => spawn_device_action(app, dispatcher, action, None),
    }
}

fn handle_device_action_input_key(
    app: &mut App,
    key: KeyEvent,
    dispatcher: &DispatchContext,
) {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.device_actions.input.clear();
        }
        KeyCode::Enter => {
            let input = app.device_actions.input.trim().to_string();
            let action = match app.input_mode {
                InputMode::DeviceText => device_actions::DeviceAction::InputText,
                InputMode::DeviceTap => device_actions::DeviceAction::Tap,
                InputMode::DeviceLocale => device_actions::DeviceAction::Locale,
                InputMode::DeviceFontScale => device_actions::DeviceAction::FontScale,
                _ => return,
            };
            app.input_mode = InputMode::Normal;
            app.device_actions.input.clear();
            spawn_device_action(app, dispatcher, action, Some(input));
        }
        KeyCode::Backspace => {
            app.device_actions.input.pop();
        }
        KeyCode::Char(c) => {
            app.device_actions.input.push(c);
        }
        _ => {}
    }
}

fn spawn_device_action(
    app: &mut App,
    dispatcher: &DispatchContext,
    action: device_actions::DeviceAction,
    input: Option<String>,
) {
    app.device_actions.running = true;
    app.device_actions.last = None;
    device_actions::spawn_action(app.device.clone(), action, input, dispatcher.tx.clone());
}

fn handle_app_data_key(app: &mut App, key: KeyEvent, dispatcher: &DispatchContext) -> bool {
    if app.app_data.preview.is_some() && app.app_data.preview_focused {
        return match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                app.app_data.preview_scroll = app.app_data.preview_scroll.saturating_add(1);
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.app_data.preview_scroll = app.app_data.preview_scroll.saturating_sub(1);
                true
            }
            KeyCode::Char(' ') => {
                app.app_data.preview_scroll = app.app_data.preview_scroll.saturating_add(12);
                true
            }
            KeyCode::Tab => {
                app.app_data.preview_focused = false;
                true
            }
            KeyCode::Backspace | KeyCode::Esc => {
                app.app_data.close_preview();
                true
            }
            _ => false,
        };
    }

    match key.code {
        KeyCode::Char('P') => {
            app.target_package_input = app.target_package.clone().unwrap_or_default();
            app.input_mode = InputMode::TargetPackage;
            true
        }
        KeyCode::Char('r') => {
            refresh_app_data(app, dispatcher, app.app_data.path.clone());
            true
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.app_data.move_down();
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.app_data.move_up();
            true
        }
        KeyCode::Enter | KeyCode::Right => {
            open_selected_app_data(app, dispatcher);
            true
        }
        KeyCode::Left => {
            if app.app_data.preview.is_some() {
                app.app_data.close_preview();
            } else if let Some(parent) = app.app_data.parent_path() {
                refresh_app_data(app, dispatcher, parent);
            }
            true
        }
        KeyCode::Backspace => {
            if app.app_data.preview.is_some() {
                app.app_data.close_preview();
            } else if let Some(parent) = app.app_data.parent_path() {
                refresh_app_data(app, dispatcher, parent);
            }
            true
        }
        KeyCode::Tab if app.app_data.preview.is_some() => {
            app.app_data.preview_focused = true;
            true
        }
        _ => false,
    }
}

fn refresh_app_data(app: &mut App, dispatcher: &DispatchContext, path: String) {
    let Some(package) = app.target_package.clone() else {
        app.flash("set target package with P".to_string(), true);
        return;
    };
    app.app_data.loading = true;
    app.app_data.last_error = None;
    crate::app_data::spawn_list(app.device.clone(), package, path, dispatcher.tx.clone());
}

fn open_selected_app_data(app: &mut App, dispatcher: &DispatchContext) {
    let Some(entry) = app.app_data.selected_entry().cloned() else {
        refresh_app_data(app, dispatcher, app.app_data.path.clone());
        return;
    };
    let Some(package) = app.target_package.clone() else {
        app.flash("set target package with P".to_string(), true);
        return;
    };
    match entry.kind {
        crate::app_data::DataEntryKind::Directory => {
            app.app_data.loading = true;
            app.app_data.last_error = None;
            crate::app_data::spawn_list(
                app.device.clone(),
                package,
                entry.path,
                dispatcher.tx.clone(),
            );
        }
        crate::app_data::DataEntryKind::File | crate::app_data::DataEntryKind::Other => {
            app.app_data.loading = true;
            app.app_data.last_error = None;
            crate::app_data::spawn_preview(
                app.device.clone(),
                package,
                entry.path,
                dispatcher.tx.clone(),
            );
        }
    }
}

fn handle_manifest_key(app: &mut App, key: KeyEvent, dispatcher: &DispatchContext) -> bool {
    match key.code {
        KeyCode::Char('P') => {
            app.target_package_input = app.target_package.clone().unwrap_or_default();
            app.input_mode = InputMode::TargetPackage;
            true
        }
        KeyCode::Char('r') => {
            refresh_manifest(app, dispatcher);
            true
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.manifest.scroll_down(1);
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.manifest.scroll_up(1);
            true
        }
        KeyCode::PageDown | KeyCode::Char(' ') => {
            app.manifest.scroll_down(12);
            true
        }
        KeyCode::PageUp => {
            app.manifest.scroll_up(12);
            true
        }
        KeyCode::Char('G') => {
            app.manifest.scroll = usize::MAX / 2;
            true
        }
        KeyCode::Char('g') => {
            app.manifest.scroll = 0;
            true
        }
        _ => false,
    }
}

fn refresh_manifest(app: &mut App, dispatcher: &DispatchContext) {
    if app.manifest.running {
        app.flash("manifest inspect already running".to_string(), false);
        return;
    }
    let Some(package) = app.target_package.clone() else {
        app.flash("set target package with P".to_string(), true);
        return;
    };
    app.manifest.running = true;
    app.manifest.scroll = 0;
    app.manifest.last = None;
    crate::manifest::spawn_inspect(app.device.clone(), package, dispatcher.tx.clone());
}

fn handle_intents_key(app: &mut App, key: KeyEvent, dispatcher: &DispatchContext) -> bool {
    match key.code {
        KeyCode::Char('/') => {
            app.deep_link_input = app.intents.url.clone();
            app.input_mode = InputMode::DeepLinkUrl;
            true
        }
        KeyCode::Char('P') => {
            app.target_package_input = app.target_package.clone().unwrap_or_default();
            app.input_mode = InputMode::TargetPackage;
            true
        }
        KeyCode::Char('T') => {
            app.intents.use_target_package = !app.intents.use_target_package;
            let mode = if app.intents.use_target_package {
                "intent package target on"
            } else {
                "intent package target off"
            };
            app.flash(mode.to_string(), false);
            true
        }
        KeyCode::Char('C') => {
            app.intents.url.clear();
            app.flash("deep link URL cleared".to_string(), false);
            true
        }
        KeyCode::Enter => {
            launch_intent(app, dispatcher);
            true
        }
        _ => false,
    }
}

fn launch_intent(app: &mut App, dispatcher: &DispatchContext) {
    if app.intents.running {
        app.flash("intent already running".to_string(), false);
        return;
    }
    let url = app.intents.url.trim().to_string();
    if url.is_empty() {
        app.flash("set deep link URL with /".to_string(), true);
        return;
    }
    let package = if app.intents.use_target_package {
        let Some(package) = app.target_package.clone() else {
            app.flash(
                "set target package with P or disable target with T".to_string(),
                true,
            );
            return;
        };
        Some(package)
    } else {
        None
    };
    app.intents.running = true;
    app.intents.last = None;
    crate::intents::spawn_launch(app.device.clone(), url, package, dispatcher.tx.clone());
}

fn app_data_event_matches_target(app: &App, event: &crate::app_data::AppDataEvent) -> bool {
    let Some(target) = app.target_package.as_deref() else {
        return false;
    };
    match event {
        crate::app_data::AppDataEvent::Listed { package, .. }
        | crate::app_data::AppDataEvent::Previewed { package, .. }
        | crate::app_data::AppDataEvent::Error { package, .. } => package == target,
    }
}

fn app_data_status(event: &crate::app_data::AppDataEvent) -> Option<(String, bool)> {
    match event {
        crate::app_data::AppDataEvent::Listed { path, entries, .. } => Some((
            format!("data: {} entries in {}", entries.len(), path),
            false,
        )),
        crate::app_data::AppDataEvent::Previewed { preview, .. } => {
            Some((format!("data: preview {}", preview.path), false))
        }
        crate::app_data::AppDataEvent::Error { path, message, .. } => {
            Some((format!("data {}: {}", path, message), true))
        }
    }
}

fn is_valid_package(package: &str) -> bool {
    !package.is_empty()
        && package
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
}

fn copy_selected_stacktrace(app: &mut App) {
    let Some(text) = app.issues.selected_stacktrace() else {
        app.flash("no stacktrace captured for selection".to_string(), true);
        return;
    };
    let bytes = text.len();
    match clipboard::copy(&text) {
        Ok(tool) => app.flash(format!("copied stacktrace ({} bytes via {})", bytes, tool), false),
        Err(e) => app.flash(format!("copy failed: {}", e), true),
    }
}

fn toggle_zoom(app: &mut App) {
    if app.zoom.is_some() {
        app.zoom = None;
        return;
    }
    if app.focus == PanelId::Shell {
        app.flash("shell panel does not support zoom".to_string(), true);
        return;
    }
    app.zoom = Some(app.focus);
}

fn toggle_mouse_mode(app: &mut App) {
    app.mouse_enabled = !app.mouse_enabled;
    let msg = if app.mouse_enabled {
        "mouse mode on: wheel scrolls, left click selects rows; Alt+m restores text selection"
    } else {
        "mouse mode off: terminal text selection restored"
    };
    app.flash(msg.to_string(), false);
}

fn start_gradle(app: &mut App, dispatcher: &DispatchContext) {
    if !app.jvm_available {
        app.flash("JDK not found; cannot run Gradle".to_string(), true);
        return;
    }
    if app.gradle.running {
        app.flash("Gradle already running".to_string(), false);
        return;
    }
    let Some(project) = app.config.gradle.project_dir.clone() else {
        app.flash(
            "set [gradle].project_dir in config.toml to run builds".to_string(),
            true,
        );
        return;
    };
    let task = app
        .config
        .gradle
        .default_task
        .clone()
        .unwrap_or_else(|| "assembleDebug".to_string());
    let jar = app
        .config
        .gradle
        .jar_path
        .clone()
        .unwrap_or_else(gradle::default_jar_path);

    if !jar.exists() {
        app.flash(
            format!("gradle-agent.jar not found at {}", jar.display()),
            true,
        );
        return;
    }

    match gradle::spawn(&jar, &project, &task, dispatcher.tx.clone()) {
        Ok(()) => {
            app.gradle.running = true;
            app.gradle.active.clear();
            app.gradle.last_error = None;
            app.gradle.last_outcome = None;
            app.focus_panel(PanelId::Gradle);
            app.flash(format!("gradle: running '{}'", task), false);
        }
        Err(e) => {
            app.flash(format!("gradle spawn failed: {}", e), true);
        }
    }
}
