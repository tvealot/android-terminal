mod adb;
mod app;
mod config;
mod dispatch;
mod gradle;
mod gradle_ui;
mod logcat;
mod logcat_ui;
mod panel;
mod theme;
mod ui;

use std::io::{self, Stdout};
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

use crate::app::App;
use crate::dispatch::{DispatchContext, Event};
use crate::panel::{by_focus_key, by_toggle_key, PanelId};

fn main() -> Result<()> {
    color_eyre::install()?;

    let cfg = config::load_config();
    let state = config::load_state();
    let jvm_available = gradle::jvm_available();
    let app = App::new(cfg, state, jvm_available);

    let dispatcher = DispatchContext::new();

    if adb::is_available() {
        let _ = adb::logcat::spawn(dispatcher.tx.clone());
    } else {
        let _ = dispatcher.tx.send(Event::Status {
            text: "adb not found in PATH — logcat disabled".to_string(),
            error: true,
        });
    }

    let mut terminal = setup_terminal()?;
    let result = run_loop(&mut terminal, app, dispatcher);
    restore_terminal(&mut terminal)?;
    result
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
) -> Result<()> {
    loop {
        for ev in dispatcher.drain() {
            match ev {
                Event::Logcat(line) => app.logcat.push(line),
                Event::Gradle(ev) => app.gradle.apply(ev),
                Event::Status { text, error } => app.flash(text, error),
            }
        }
        app.tick_status();

        let theme = theme::by_name(&app.config.ui.theme);
        terminal.draw(|f| ui::render(f, &app, theme))?;

        if event::poll(Duration::from_millis(100))? {
            if let CEvent::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(&mut app, key, &dispatcher);
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent, dispatcher: &DispatchContext) {
    if key.modifiers.contains(KeyModifiers::ALT) {
        if let KeyCode::Char(c) = key.code {
            if let Some(id) = by_toggle_key(c) {
                app.toggle_panel(id);
                return;
            }
        }
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc if !app.show_help => {
            app.should_quit = true;
        }
        KeyCode::Esc if app.show_help => {
            app.show_help = false;
        }
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
        }
        KeyCode::Char('r') => {
            start_gradle(app, dispatcher);
        }
        KeyCode::Char(c) => {
            if let Some(id) = by_focus_key(c) {
                app.focus_panel(id);
            }
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
