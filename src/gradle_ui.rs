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

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(3)])
        .split(inner);

    render_active(f, chunks[0], app, theme);
    render_history(f, chunks[1], app, theme);
}

fn render_active(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let title = Line::from(vec![Span::styled(
        format!(" active tasks ({}) ", app.gradle.active.len()),
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
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
        lines.push(Line::from(Span::styled(msg, Style::default().fg(theme.muted))));
    } else {
        for task in app.gradle.active.values() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", spinner),
                    Style::default().fg(theme.warn),
                ),
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
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    ))];
    all.extend(history);
    f.render_widget(Paragraph::new(all), area);
}

fn spinner_char() -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let idx = (chrono::Local::now().timestamp_millis() / 100) as usize % FRAMES.len();
    FRAMES[idx]
}
