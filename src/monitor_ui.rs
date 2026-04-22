use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::panel::{def, PANELS};
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let block = Block::default()
        .title(Span::styled(
            " monitor ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let gradle_status = if app.gradle.running {
        ("running".to_string(), theme.warn)
    } else if let Some(outcome) = &app.gradle.last_outcome {
        let color = if outcome == "SUCCESS" {
            theme.success
        } else {
            theme.error
        };
        (format!("last build {}", outcome.to_lowercase()), color)
    } else if let Some(err) = &app.gradle.last_error {
        (short_text(err, inner.width as usize), theme.error)
    } else {
        ("idle".to_string(), theme.muted)
    };

    let mut lines = vec![
        section_title(" environment ", theme),
        kv(
            "adb",
            status_word(app.adb_available),
            if app.adb_available {
                theme.success
            } else {
                theme.error
            },
            theme,
        ),
        kv(
            "java",
            status_word(app.jvm_available),
            if app.jvm_available {
                theme.success
            } else {
                theme.error
            },
            theme,
        ),
        kv("focus", def(app.focus).name.to_string(), theme.fg, theme),
        kv(
            "visible",
            format!("{}/{} panels", app.visible.len(), PANELS.len()),
            theme.fg,
            theme,
        ),
        kv(
            "logcat",
            format!("{} buffered lines", app.logcat.lines.len()),
            theme.fg,
            theme,
        ),
        kv("gradle", gradle_status.0, gradle_status.1, theme),
        Line::from(""),
        section_title(format!(" devices ({}) ", app.devices.len()), theme),
    ];

    if !app.adb_available {
        lines.push(message_line("adb not found in PATH", theme.error));
    } else if app.devices.is_empty() {
        lines.push(message_line("no connected devices", theme.muted));
    } else {
        for serial in &app.devices {
            lines.push(Line::from(vec![
                Span::styled("  device  ", Style::default().fg(theme.muted)),
                Span::styled(
                    short_text(serial, inner.width as usize),
                    Style::default().fg(theme.fg),
                ),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn status_word(ok: bool) -> String {
    if ok {
        "ready".to_string()
    } else {
        "missing".to_string()
    }
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

fn short_text(text: &str, max: usize) -> String {
    if max <= 3 || text.len() <= max {
        return text.to_string();
    }
    format!("{}...", &text[..max - 3])
}
