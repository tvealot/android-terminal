use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::logcat::LogLine;
use crate::theme::Theme;

const KEYWORDS: &[&str] = &[
    "okhttp",
    "retrofit",
    "http",
    "https",
    "socket",
    "websocket",
    "grpc",
    "apollo",
    "dns",
    "ssl",
    "tls",
    "request",
    "response",
];

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let block = Block::default()
        .title(Span::styled(
            " network ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if !app.adb_available {
        let lines = vec![
            section_title(" live capture ", theme),
            message_line("adb not found in PATH", theme.error),
            message_line("network panel follows logcat and needs adb", theme.muted),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let matches: Vec<&LogLine> = app
        .logcat
        .lines
        .iter()
        .filter(|line| is_network_line(line))
        .collect();

    let mut lines = vec![
        section_title(" live capture ", theme),
        kv(
            "buffer",
            format!("{} total lines", app.logcat.lines.len()),
            theme.fg,
            theme,
        ),
        kv(
            "matches",
            format!("{} network-like lines", matches.len()),
            theme.fg,
            theme,
        ),
    ];

    if matches.is_empty() {
        lines.push(Line::from(""));
        lines.push(message_line(
            "waiting for okhttp/http/socket/dns logcat lines",
            theme.muted,
        ));
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let available = inner.height.saturating_sub(lines.len() as u16) as usize;
    let start = matches.len().saturating_sub(available);
    for line in &matches[start..] {
        lines.push(render_line(line, inner.width as usize, theme));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn is_network_line(line: &LogLine) -> bool {
    let tag = line.tag.to_lowercase();
    let message = line.message.to_lowercase();
    KEYWORDS
        .iter()
        .any(|needle| tag.contains(needle) || message.contains(needle))
}

fn render_line(line: &LogLine, width: usize, theme: &Theme) -> Line<'static> {
    let message_width = width.saturating_sub(28);
    Line::from(vec![
        Span::styled(
            format!("{} ", line.timestamp),
            Style::default().fg(theme.muted),
        ),
        Span::styled(
            format!("{:<14} ", shrink(&line.tag, 14)),
            Style::default().fg(theme.accent),
        ),
        Span::styled(
            shrink(&line.message, message_width),
            Style::default().fg(theme.fg),
        ),
    ])
}

fn section_title<T: Into<String>>(title: T, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        title.into(),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    ))
}

fn kv(label: &str, value: String, color: Color, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), Style::default().fg(theme.muted)),
        Span::raw(" "),
        Span::styled(value, Style::default().fg(color)),
    ])
}

fn message_line(text: &str, color: Color) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), Style::default().fg(color)))
}

fn shrink(text: &str, max: usize) -> String {
    if max <= 3 || text.len() <= max {
        return text.to_string();
    }
    format!("{}...", &text[..max - 3])
}
