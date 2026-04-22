use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Sparkline};
use ratatui::Frame;

use crate::app::App;
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

    let Some(latest) = app.monitor.latest() else {
        let msg = if app.monitor.last_error.is_some() {
            "monitor error — check adb"
        } else {
            "waiting for first sample..."
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(msg, Style::default().fg(theme.muted)))),
            inner,
        );
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // battery line
            Constraint::Length(1), // battery gauge
            Constraint::Length(1), // mem line
            Constraint::Length(1), // mem gauge
            Constraint::Min(1),    // sparkline history
        ])
        .split(inner);

    let battery_label = Paragraph::new(Line::from(vec![
        Span::styled("battery  ", Style::default().fg(theme.accent)),
        Span::styled(
            format!("{}%  ", latest.battery_percent),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:.1}°C", latest.battery_temp_c),
            Style::default().fg(theme.muted),
        ),
    ]));
    f.render_widget(battery_label, chunks[0]);

    let battery_gauge = Gauge::default()
        .gauge_style(Style::default().fg(battery_color(latest.battery_percent, theme)))
        .ratio((latest.battery_percent as f64 / 100.0).clamp(0.0, 1.0))
        .label("");
    f.render_widget(battery_gauge, chunks[1]);

    let mem_label = Paragraph::new(Line::from(vec![
        Span::styled("memory   ", Style::default().fg(theme.accent)),
        Span::styled(
            format!("{:.1}%  ", latest.mem_used_percent()),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "{} MB / {} MB",
                latest.mem_used_kb() / 1024,
                latest.mem_total_kb / 1024
            ),
            Style::default().fg(theme.muted),
        ),
    ]));
    f.render_widget(mem_label, chunks[2]);

    let mem_gauge = Gauge::default()
        .gauge_style(Style::default().fg(theme.accent))
        .ratio((latest.mem_used_percent() as f64 / 100.0).clamp(0.0, 1.0))
        .label("");
    f.render_widget(mem_gauge, chunks[3]);

    if chunks[4].height >= 2 {
        let history: Vec<u64> = app
            .monitor
            .samples
            .iter()
            .map(|s| s.mem_used_percent() as u64)
            .collect();
        let spark = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .title(Span::styled(
                        " mem % history ",
                        Style::default().fg(theme.muted),
                    ))
                    .border_style(Style::default().fg(theme.surface)),
            )
            .data(&history)
            .max(100)
            .bar_set(symbols::bar::NINE_LEVELS)
            .style(Style::default().fg(theme.accent));
        f.render_widget(spark, chunks[4]);
    }
}

fn battery_color(level: u8, theme: &Theme) -> ratatui::style::Color {
    if level < 20 {
        theme.error
    } else if level < 50 {
        theme.warn
    } else {
        theme.success
    }
}
