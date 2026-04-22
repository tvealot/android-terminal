use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::logcat::LogLevel;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let level = app.logcat.min_level;
    let level_part = if level == crate::logcat::LogLevel::Verbose {
        String::new()
    } else {
        format!(" [{}+]", level.short())
    };
    let pkg_part = match (&app.logcat.filter_package, app.logcat.filter_pid) {
        (Some(p), Some(pid)) => format!(" pkg={} pid={}", p, pid),
        _ => String::new(),
    };
    let paused_part = if app.logcat.paused { " [PAUSED]" } else { "" };
    let filter_hint = if app.input_mode == crate::app::InputMode::LogcatFilter {
        format!(" logcat{}{}{} — filter: {}_ ", level_part, pkg_part, paused_part, app.logcat.filter)
    } else if !app.logcat.filter.is_empty() {
        format!(" logcat{}{}{} — filter: {} ", level_part, pkg_part, paused_part, app.logcat.filter)
    } else {
        format!(" logcat{}{}{} ", level_part, pkg_part, paused_part)
    };
    let block = Block::default()
        .title(Span::styled(
            filter_hint,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let height = inner.height as usize;
    let collected: Vec<&crate::logcat::LogLine> = app.logcat.visible().collect();
    let start = collected.len().saturating_sub(height);
    let lines: Vec<Line> = collected[start..]
        .iter()
        .map(|line| {
            let (level_str, level_color) = match line.level {
                LogLevel::Verbose => ("V", theme.muted),
                LogLevel::Debug => ("D", Color::Cyan),
                LogLevel::Info => ("I", theme.success),
                LogLevel::Warn => ("W", theme.warn),
                LogLevel::Error => ("E", theme.error),
                LogLevel::Fatal => ("F", theme.error),
            };
            Line::from(vec![
                Span::styled(
                    format!("{} ", &line.timestamp),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(
                    format!("{} ", level_str),
                    Style::default().fg(level_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<20} ", truncate(&line.tag, 20)),
                    Style::default().fg(theme.accent),
                ),
                Span::styled(line.message.clone(), Style::default().fg(theme.fg)),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
