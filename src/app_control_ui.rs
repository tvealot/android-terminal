use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::app_control::ACTIONS;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let mut block = Block::default()
        .title(Span::styled(
            " app ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if focused {
        block = block.title_bottom(Line::from(vec![
            Span::styled(" P", Style::default().fg(theme.warn)),
            Span::styled(" package ", Style::default().fg(theme.muted)),
            Span::styled("───", Style::default().fg(border_color)),
            Span::styled(" ↩", Style::default().fg(theme.warn)),
            Span::styled(" run ", Style::default().fg(theme.muted)),
            Span::styled("───", Style::default().fg(border_color)),
            Span::styled(" !", Style::default().fg(theme.warn)),
            Span::styled(" confirm ", Style::default().fg(theme.muted)),
        ]));
    }

    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(inner);

    render_actions(f, cols[0], app, theme, focused);
    render_result(f, cols[1], app, theme);
}

fn render_actions(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let target = app.target_package.as_deref().unwrap_or("(unset)");
    let mut lines = vec![
        Line::from(vec![
            Span::styled("target  ", Style::default().fg(theme.muted)),
            Span::styled(target.to_string(), Style::default().fg(theme.accent)),
        ]),
        Line::from(""),
    ];

    for (i, action) in ACTIONS.iter().enumerate() {
        let selected = focused && i == app.app_control.selected;
        let pending = app.app_control.pending_confirm == Some(*action);
        let label_style = if selected {
            Style::default()
                .fg(theme.bg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if action.destructive() {
            Style::default().fg(theme.warn).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };
        let marker = if selected { ">" } else { " " };
        let confirm = if pending { "  press !" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(format!(" {marker} "), label_style),
            Span::styled(format!("{:<13}", action.label()), label_style),
            Span::styled(confirm.to_string(), Style::default().fg(theme.warn)),
        ]));
        if area.height > 8 {
            lines.push(Line::from(Span::styled(
                format!("    {}", action.description()),
                Style::default().fg(theme.muted),
            )));
        }
    }

    if app.app_control.running {
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
    if let Some(result) = &app.app_control.last {
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
                format!("  {}", result.package),
                Style::default().fg(theme.muted),
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
            .take(inner.height.saturating_sub(3) as usize)
        {
            lines.push(Line::from(Span::styled(
                truncate(line, inner.width as usize),
                Style::default().fg(theme.fg),
            )));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Select an action and press Enter.",
            Style::default().fg(theme.muted),
        )));
        lines.push(Line::from(Span::styled(
            "Clear data requires a second `!` confirmation.",
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
