use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let mut block = Block::default()
        .title(Span::styled(
            " intents ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if focused {
        block = block.title_bottom(Line::from(vec![
            Span::styled(" /", Style::default().fg(theme.warn)),
            Span::styled(" url ", Style::default().fg(theme.muted)),
            Span::styled("───", Style::default().fg(border_color)),
            Span::styled(" P", Style::default().fg(theme.warn)),
            Span::styled(" package ", Style::default().fg(theme.muted)),
            Span::styled("───", Style::default().fg(border_color)),
            Span::styled(" T", Style::default().fg(theme.warn)),
            Span::styled(" target ", Style::default().fg(theme.muted)),
            Span::styled("───", Style::default().fg(border_color)),
            Span::styled(" ↩", Style::default().fg(theme.warn)),
            Span::styled(" launch ", Style::default().fg(theme.muted)),
        ]));
    }

    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let target = app.target_package.as_deref().unwrap_or("(unset)");
    let package_mode = if app.intents.use_target_package {
        "explicit"
    } else {
        "resolver"
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("url      ", Style::default().fg(theme.muted)),
            Span::styled(
                if app.intents.url.is_empty() {
                    "(empty)".to_string()
                } else {
                    app.intents.url.clone()
                },
                Style::default().fg(theme.accent),
            ),
        ]),
        Line::from(vec![
            Span::styled("target   ", Style::default().fg(theme.muted)),
            Span::styled(target.to_string(), Style::default().fg(theme.fg)),
        ]),
        Line::from(vec![
            Span::styled("mode     ", Style::default().fg(theme.muted)),
            Span::styled(package_mode.to_string(), Style::default().fg(theme.warn)),
        ]),
    ];

    if app.intents.running {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "launching intent...",
            Style::default().fg(theme.warn),
        )));
    }

    if let Some(result) = &app.intents.last {
        let color = if result.success {
            theme.success
        } else {
            theme.error
        };
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("last     ", Style::default().fg(theme.muted)),
            Span::styled(result.summary.clone(), Style::default().fg(color)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("url      ", Style::default().fg(theme.muted)),
            Span::styled(result.url.clone(), Style::default().fg(theme.fg)),
        ]));
        if let Some(package) = &result.package {
            lines.push(Line::from(vec![
                Span::styled("package  ", Style::default().fg(theme.muted)),
                Span::styled(package.clone(), Style::default().fg(theme.fg)),
            ]));
        }
        for line in result
            .output
            .lines()
            .take(inner.height.saturating_sub(8) as usize)
        {
            lines.push(Line::from(Span::styled(
                truncate(line, inner.width as usize),
                Style::default().fg(theme.fg),
            )));
        }
    } else if !app.intents.history.is_empty() && inner.height > 8 {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "recent",
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )));
        for item in app.intents.history.iter().take(4) {
            lines.push(Line::from(Span::styled(
                truncate(item, inner.width as usize),
                Style::default().fg(theme.fg),
            )));
        }
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press / to enter a URL, then Enter to run `am start -a VIEW -d`.",
            Style::default().fg(theme.muted),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{head}…")
    }
}
