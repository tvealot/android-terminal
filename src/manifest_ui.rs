use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let mut block = Block::default()
        .title(Span::styled(
            " manifest ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if focused {
        block = block.title_bottom(Line::from(vec![
            Span::styled(" P", Style::default().fg(theme.warn)),
            Span::styled(" package ", Style::default().fg(theme.muted)),
            Span::styled("---", Style::default().fg(border_color)),
            Span::styled(" r", Style::default().fg(theme.warn)),
            Span::styled("efresh ", Style::default().fg(theme.muted)),
            Span::styled("---", Style::default().fg(border_color)),
            Span::styled(" j/k", Style::default().fg(theme.warn)),
            Span::styled(" scroll ", Style::default().fg(theme.muted)),
        ]));
    }

    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = Vec::new();
    let target = app.target_package.as_deref().unwrap_or("(unset)");
    lines.push(Line::from(vec![
        Span::styled("target  ", Style::default().fg(theme.muted)),
        Span::styled(truncate(target, inner.width as usize), Style::default().fg(theme.accent)),
    ]));

    if app.manifest.running {
        lines.push(Line::from(Span::styled(
            "inspecting installed APK...",
            Style::default().fg(theme.warn),
        )));
    } else if app.target_package.is_none() {
        lines.push(Line::from(Span::styled(
            "Press P to set a target package.",
            Style::default().fg(theme.muted),
        )));
    } else if let Some(report) = &app.manifest.last {
        let color = if report.success {
            theme.success
        } else {
            theme.error
        };
        lines.push(Line::from(Span::styled(
            report.summary.clone(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(vec![
            Span::styled("package ", Style::default().fg(theme.muted)),
            Span::styled(report.package.clone(), Style::default().fg(theme.fg)),
        ]));
        lines.push(Line::from(""));
        for line in report.output.lines() {
            lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(theme.fg))));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Press r to inspect installed APK path, version, manifest entries, and deep links.",
            Style::default().fg(theme.muted),
        )));
    }

    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    let scroll = app.manifest.scroll.min(max_scroll);
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        inner,
    );

    if max_scroll > 0 {
        let mut state = ScrollbarState::new(max_scroll).position(scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme.muted))
            .track_style(Style::default().fg(theme.surface));
        f.render_stateful_widget(scrollbar, inner, &mut state);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else if max <= 3 {
        ".".repeat(max)
    } else {
        let head: String = s.chars().take(max - 3).collect();
        format!("{head}...")
    }
}
