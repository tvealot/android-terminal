mod adb;
mod app;
mod config;
mod dispatch;
mod files;
mod files_ui;
mod gradle;
mod gradle_ui;
mod issues;
mod issues_ui;
mod logcat;
mod logcat_ui;
mod monitor;
mod monitor_ui;
mod network_ui;
mod panel;
mod processes;
mod processes_ui;
mod theme;
mod ui;

use std::io::{self, Stdout};
use std::process::Child;
use std::time::Duration;

use color_eyre::eyre::{Result, WrapErr};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyEvent,
    KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::{App, InputMode};
use crate::dispatch::{DispatchContext, Event};
use crate::panel::{by_focus_key, by_toggle_key, PanelId};

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
    let app = App::new(cfg, state, jvm_available, adb_available, device.clone());

    let dispatcher = DispatchContext::new();
    let mut runtime = Runtime { logcat_child: None };

    if adb_available {
        runtime.restart_logcat(&app, &dispatcher);
        monitor::spawn_poller(device.clone(), dispatcher.tx.clone());
        processes::spawn_poller(device.clone(), dispatcher.tx.clone());
        adb::devices::spawn_poller(dispatcher.tx.clone());
    } else {
        let _ = dispatcher.tx.send(Event::Status {
            text: "adb not found in PATH — logcat/monitor disabled".to_string(),
            error: true,
        });
    }

    let mut terminal = setup_terminal()?;
    let result = run_loop(&mut terminal, app, dispatcher, runtime);
    result
        .and(restore_terminal(&mut terminal))
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode().wrap_err("enable_raw_mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).wrap_err("enter alt screen")?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).wrap_err("terminal init")
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
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
                Event::Monitor(sample) => app.monitor.push(sample),
                Event::Processes(procs) => app.processes.replace(procs),
                Event::Devices(list) => app.devices = list,
                Event::Status { text, error } => app.flash(text, error),
            }
        }
        app.tick_status();

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
        InputMode::Normal => {}
    }

    // Device selector overlay: consumes keys while open.
    if let Some(idx) = app.device_selector {
        return handle_device_selector(app, key, idx, dispatcher, runtime);
    }

    if key.modifiers.contains(KeyModifiers::ALT) {
        if let KeyCode::Char(c) = key.code {
            if let Some(id) = by_toggle_key(c) {
                app.toggle_panel(id);
                return;
            }
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

    match key.code {
        KeyCode::Esc if app.show_help => {
            app.show_help = false;
        }
        KeyCode::Esc if app.focus == PanelId::Issues && app.issues.expanded.is_some() => {
            app.issues.close_detail();
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
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
        KeyCode::Char('C') if app.focus == PanelId::Issues => {
            app.issues.clear();
            app.flash("issues cleared".to_string(), false);
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
        KeyCode::Char(c) => {
            if let Some(id) = by_focus_key(c) {
                app.focus_panel(id);
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
        }
        KeyCode::Char(c) => {
            app.logcat.filter.push(c);
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
