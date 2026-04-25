use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::device_actions::ACTIONS;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let mut block = Block::default()
        .title(Span::styled(
            " device actions ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if focused {
        block = block.title_bottom(Line::from(vec![
            Span::styled(" ↩", Style::default().fg(theme.warn)),
            Span::styled(" run ", Style::default().fg(theme.muted)),
            Span::styled("───", Style::default().fg(border_color)),
            Span::styled(" j/k", Style::default().fg(theme.warn)),
            Span::styled(" move ", Style::default().fg(theme.muted)),
        ]));
    }

    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(inner);

    render_actions(f, cols[0], app, theme, focused);
    render_result(f, cols[1], app, theme);
}

fn render_actions(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("device  ", Style::default().fg(theme.muted)),
            Span::styled(
                app.current_device()
                    .unwrap_or_else(|| "(default)".to_string()),
                Style::default().fg(theme.accent),
            ),
        ]),
        Line::from(""),
    ];

    let available = area.height.saturating_sub(2) as usize;
    let selected = app.device_actions.selected;
    let start = if selected >= available {
        selected + 1 - available
    } else {
        0
    };
    for (i, action) in ACTIONS.iter().enumerate().skip(start).take(available) {
        let selected = focused && i == app.device_actions.selected;
        let label_style = if selected {
            Style::default()
                .fg(theme.bg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if action.needs_input() {
            Style::default().fg(theme.warn)
        } else {
            Style::default().fg(theme.fg)
        };
        let marker = if selected { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(format!(" {marker} "), label_style),
            Span::styled(action.label(), label_style),
        ]));
    }

    if app.device_actions.running {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "running adb command...",
            Style::default().fg(theme.warn),
        )));
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn render_result(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme.surface));
    let inner = separator.inner(area);
    f.render_widget(separator, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = Vec::new();
    let action = app.device_actions.selected_action();
    lines.push(Line::from(vec![
        Span::styled("selected  ", Style::default().fg(theme.muted)),
        Span::styled(
            action.label(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        action.description(),
        Style::default().fg(theme.muted),
    )));
    lines.push(Line::from(""));

    if let Some(result) = &app.device_actions.last {
        let color = if result.success {
            theme.success
        } else {
            theme.error
        };
        lines.push(Line::from(vec![
            Span::styled(
                result.action.label(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if result.success { "  ok" } else { "  failed" },
                Style::default().fg(color),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            result.summary.clone(),
            Style::default().fg(color),
        )));
        lines.push(Line::from(""));
        for line in result
            .output
            .lines()
            .take(inner.height.saturating_sub(lines.len() as u16) as usize)
        {
            lines.push(Line::from(Span::styled(
                truncate(line, inner.width as usize),
                Style::default().fg(theme.fg),
            )));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Press Enter to run the selected action.",
            Style::default().fg(theme.muted),
        )));
        lines.push(Line::from(Span::styled(
            "Input actions prompt in the footer.",
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
