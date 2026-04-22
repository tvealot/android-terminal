use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::issues::IssueKind;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let count = app.issues.issues.len();
    let block = Block::default()
        .title(Span::styled(
            format!(" issues ({}) ", count),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if count == 0 {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no crashes or ANRs detected",
                Style::default().fg(theme.muted),
            ))),
            inner,
        );
        return;
    }

    let height = inner.height.saturating_sub(1) as usize;
    let offset = app.issues.selected.saturating_sub(height.saturating_sub(1));
    let rows: Vec<Line> = app
        .issues
        .issues
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(i, issue)| {
            let sel = i == app.issues.selected && focused;
            let row_style = if sel {
                Style::default().fg(theme.bg).bg(theme.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };
            let kind_color = match issue.kind {
                IssueKind::Crash => theme.error,
                IssueKind::Anr => theme.warn,
                IssueKind::Tombstone => theme.error,
            };
            let count_marker = if issue.count > 1 {
                format!(" ×{}", issue.count)
            } else {
                String::new()
            };
            Line::from(vec![
                Span::styled(
                    format!(" {:<7}", issue.kind.label()),
                    Style::default().fg(kind_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}  ", issue.timestamp),
                    Style::default().fg(theme.muted),
                ),
                Span::styled(format!("pid={:<6} ", issue.pid), Style::default().fg(theme.muted)),
                Span::styled(format!("{:<18} ", truncate(&issue.tag, 18)), Style::default().fg(theme.accent)),
                Span::styled(
                    format!("{}{}", truncate(&issue.excerpt, inner.width as usize / 2), count_marker),
                    row_style,
                ),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(rows).wrap(Wrap { trim: false }), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 || s.len() <= max {
        return s.to_string();
    }
    format!("{}…", &s[..max.saturating_sub(1)])
}
