use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let count = app.processes.processes.len();
    let block = Block::default()
        .title(Span::styled(
            format!(" processes ({}) ", count),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if count == 0 {
        let msg = if app.processes.last_error.is_some() {
            "processes error — check adb"
        } else {
            "waiting for first sample..."
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(msg, Style::default().fg(theme.muted)))),
            inner,
        );
        return;
    }

    let height = inner.height.saturating_sub(1) as usize;
    let header = Line::from(vec![
        Span::styled(
            format!("{:>7} {:<10} {:>9}  {}", "PID", "USER", "RSS", "NAME"),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
    ]);

    let rows: Vec<Line> = app
        .processes
        .processes
        .iter()
        .take(height)
        .enumerate()
        .map(|(i, p)| {
            let style = if i == app.processes.selected && focused {
                Style::default().fg(theme.bg).bg(theme.accent)
            } else {
                Style::default().fg(theme.fg)
            };
            Line::from(Span::styled(
                format!(
                    "{:>7} {:<10} {:>7} MB  {}",
                    p.pid,
                    truncate(&p.user, 10),
                    p.rss_kb / 1024,
                    truncate(&p.name, (inner.width as usize).saturating_sub(32))
                ),
                style,
            ))
        })
        .collect();

    let mut all = vec![header];
    all.extend(rows);
    f.render_widget(Paragraph::new(all), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.len() <= max {
        s.to_string()
    } else {
        let cutoff = max.saturating_sub(1);
        format!("{}…", &s[..cutoff])
    }
}
