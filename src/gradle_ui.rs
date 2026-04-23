use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let block = Block::default()
        .title(Span::styled(
            " gradle ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if !app.jvm_available {
        let msg = Paragraph::new(Line::from(Span::styled(
            "JDK not found. Install Java 17+ and configure gradle.jar_path.",
            Style::default().fg(theme.warn),
        )));
        f.render_widget(msg, inner);
        return;
    }

    let host_h = (app.gradle.host_procs.len() as u16 + 1).min(6);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(host_h.max(1)),
            Constraint::Min(3),
        ])
        .split(inner);

    render_active(f, chunks[0], app, theme);
    render_host(f, chunks[1], app, theme);
    render_history(f, chunks[2], app, theme);
}

fn render_host(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let title = Line::from(Span::styled(
        format!(" host gradle ({}) ", app.gradle.host_procs.len()),
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    ));
    let mut lines: Vec<Line> = vec![title];
    if app.gradle.host_procs.is_empty() {
        lines.push(Line::from(Span::styled(
            "no external gradle processes",
            Style::default().fg(theme.muted),
        )));
    } else {
        for p in &app.gradle.host_procs {
            let mb = p.rss_kb as f64 / 1024.0;
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<8} ", p.kind),
                    Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("pid {:<6} ", p.pid), Style::default().fg(theme.fg)),
                Span::styled(
                    format!("cpu {:>5.1}%  ", p.cpu),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(
                    format!("rss {:>6.0} MB", mb),
                    Style::default().fg(theme.muted),
                ),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn render_active(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let title = Line::from(vec![Span::styled(
        format!(" active tasks ({}) ", app.gradle.active.len()),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )]);
    let spinner = spinner_char();

    let mut lines: Vec<Line> = vec![title];
    if app.gradle.active.is_empty() {
        let msg = if app.gradle.running {
            "waiting for tasks..."
        } else if let Some(outcome) = &app.gradle.last_outcome {
            if outcome == "SUCCESS" {
                "build finished: SUCCESS"
            } else {
                "build finished with failure"
            }
        } else {
            "idle. press 'r' to run default task"
        };
        lines.push(Line::from(Span::styled(
            msg,
            Style::default().fg(theme.muted),
        )));
    } else {
        for task in app.gradle.active.values() {
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", spinner), Style::default().fg(theme.warn)),
                Span::styled(task.path.clone(), Style::default().fg(theme.fg)),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn render_history(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let height = area.height.saturating_sub(1) as usize;
    let history: Vec<Line> = app
        .gradle
        .completed
        .iter()
        .rev()
        .take(height)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|task| {
            let color = match task.outcome.as_str() {
                "SUCCESS" => theme.success,
                "UP_TO_DATE" | "FROM_CACHE" | "SKIPPED" => theme.muted,
                "FAILED" => theme.error,
                _ => theme.fg,
            };
            Line::from(vec![
                Span::styled(
                    format!("{:<12} ", task.outcome),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:>8} ms  ", task.duration_ms),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(task.path.clone(), Style::default().fg(theme.fg)),
            ])
        })
        .collect();
    let mut all = vec![Line::from(Span::styled(
        " history ",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    ))];
    all.extend(history);
    f.render_widget(Paragraph::new(all), area);
}

fn spinner_char() -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let idx = (chrono::Local::now().timestamp_millis() / 100) as usize % FRAMES.len();
    FRAMES[idx]
}
