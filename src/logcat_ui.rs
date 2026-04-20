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
    let block = Block::default()
        .title(Span::styled(
            " logcat ",
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
