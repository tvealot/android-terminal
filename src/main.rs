mod adb;
mod app;
mod app_control;
mod app_control_ui;
mod app_data;
mod app_data_ui;
mod clipboard;
mod config;
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
    self, Event as CEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
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
    let jvm_available = gradle::jvm_available();
    let adb_available = adb::is_available();
    let device = adb::new_handle();
    let fps_package = fps::new_package_handle();
    let perf_package = perf::new_package_handle();
    let app = App::new(
        cfg,
        state,
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
    loop {
        for ev in dispatcher.drain() {
            match ev {
                Event::Logcat(line) => {
                    app.issues.detect(&line);
                    app.logcat.push(line);
                }
                Event::Gradle(ev) => app.gradle.apply(ev),
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
            if let CEvent::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(&mut app, key, &dispatcher, &mut runtime);
                }
            }
        }

        if app.should_quit {
            if let Some(mut child) = runtime.logcat_child.take() {
                let _ = child.kill();
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
        InputMode::LayoutEdit => {
            return handle_layout_editor_key(app, key);
        }
        InputMode::Normal => {}
    }

    let raw = key;
    let key = keymap::normalize(key);

    // Project picker overlay: consumes keys while open.
    if app.project_picker.is_some() {
        return handle_project_picker_key(app, key);
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

fn handle_project_picker_key(app: &mut App, key: KeyEvent) {
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
                app.apply_project_dir(path);
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
