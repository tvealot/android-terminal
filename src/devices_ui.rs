use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let title = format!(" devices ({}) ", app.devices.len());
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.devices.is_empty() {
        let p = Paragraph::new(Line::from(Span::styled(
            " no devices connected",
            Style::default().fg(theme.muted),
        )));
        f.render_widget(p, inner);
        return;
    }

    let current = app.current_device();
    let selected = app.devices_selected.min(app.devices.len().saturating_sub(1));
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(
            " {:<3} {:<22} {:<8} {:<20} {:<10} {}",
            "sel", "serial", "state", "model", "android", "bat"
        ),
        Style::default().fg(theme.muted).add_modifier(Modifier::BOLD),
    )));
    for (i, d) in app.devices.iter().enumerate() {
        let is_current = Some(&d.serial) == current.as_ref();
        let marker = if is_current { "●" } else { " " };
        let state_color = if d.is_ready() { theme.success } else { theme.warn };
        let row_style = if i == selected && focused {
            Style::default().fg(theme.bg).bg(theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };
        let model = d.model.clone().unwrap_or_else(|| "-".to_string());
        let release = d.release.clone().unwrap_or_else(|| "-".to_string());
        let sdk = d.sdk.clone().unwrap_or_else(|| "?".to_string());
        let android = format!("{} (sdk {})", release, sdk);
        let bat = d
            .battery
            .map(|b| format!("{}%", b))
            .unwrap_or_else(|| "-".to_string());
        let bat_color = battery_color(d.battery, theme);
        lines.push(Line::from(vec![
            Span::styled(format!(" {:<3} ", marker), row_style),
            Span::styled(format!("{:<22} ", truncate(&d.serial, 22)), row_style),
            Span::styled(format!("{:<8} ", d.state), Style::default().fg(state_color)),
            Span::styled(format!("{:<20} ", truncate(&model, 20)), row_style),
            Span::styled(format!("{:<10} ", truncate(&android, 10)), row_style),
            Span::styled(bat, Style::default().fg(bat_color)),
        ]));
    }
    if focused {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " j/k: move   Enter: switch device",
            Style::default().fg(theme.muted),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn battery_color(level: Option<u8>, theme: &Theme) -> ratatui::style::Color {
    match level {
        Some(l) if l < 20 => theme.error,
        Some(l) if l < 50 => theme.warn,
        Some(_) => theme.success,
        None => theme.muted,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut)
    }
}
