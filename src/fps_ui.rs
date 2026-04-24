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
    let pkg = app.fps.current_package();
    let title = match &pkg {
        Some(p) => format!(" fps · {} ", p),
        None => " fps ".to_string(),
    };
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if pkg.is_none() {
        let msg = Paragraph::new(vec![
            Line::from(Span::styled(
                "no package selected",
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "press P to enter a package (e.g. com.example.app)",
                Style::default().fg(theme.muted),
            )),
        ]);
        f.render_widget(msg, inner);
        return;
    }

    let Some(latest) = app.fps.latest() else {
        let msg = if app.fps.last_error.is_some() {
            "fps error — check adb / package running"
        } else {
            "waiting for first sample…"
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
            Constraint::Length(1), // summary
            Constraint::Length(1), // jank gauge
            Constraint::Length(1), // percentiles
            Constraint::Length(1), // counters
            Constraint::Min(1),    // sparkline
        ])
        .split(inner);

    let summary = Paragraph::new(Line::from(vec![
        Span::styled("frames  ", Style::default().fg(theme.accent)),
        Span::styled(
            format!("{}", latest.total_frames),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("jank  ", Style::default().fg(theme.accent)),
        Span::styled(
            format!("{} ({:.2}%)", latest.janky_frames, latest.janky_percent),
            Style::default()
                .fg(jank_color(latest.janky_percent, theme))
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    f.render_widget(summary, chunks[0]);

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(jank_color(latest.janky_percent, theme)))
        .ratio((latest.janky_percent as f64 / 50.0).clamp(0.0, 1.0))
        .label(Span::styled(
            format!("jank {:.2}%  (gauge scale 0–50%)", latest.janky_percent),
            Style::default().fg(theme.bg),
        ));
    f.render_widget(gauge, chunks[1]);

    let percentiles = Paragraph::new(Line::from(vec![
        Span::styled("p50 ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{:.0}ms ", latest.p50_ms),
            Style::default().fg(pct_color(latest.p50_ms, theme)),
        ),
        Span::styled("p90 ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{:.0}ms ", latest.p90_ms),
            Style::default().fg(pct_color(latest.p90_ms, theme)),
        ),
        Span::styled("p95 ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{:.0}ms ", latest.p95_ms),
            Style::default().fg(pct_color(latest.p95_ms, theme)),
        ),
        Span::styled("p99 ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{:.0}ms", latest.p99_ms),
            Style::default().fg(pct_color(latest.p99_ms, theme)),
        ),
    ]));
    f.render_widget(percentiles, chunks[2]);

    let counters = Paragraph::new(Line::from(vec![
        Span::styled("vsync ", Style::default().fg(theme.muted)),
        Span::styled(format!("{} ", latest.missed_vsync), Style::default().fg(theme.fg)),
        Span::styled("input ", Style::default().fg(theme.muted)),
        Span::styled(format!("{} ", latest.high_input_latency), Style::default().fg(theme.fg)),
        Span::styled("ui ", Style::default().fg(theme.muted)),
        Span::styled(format!("{} ", latest.slow_ui), Style::default().fg(theme.fg)),
        Span::styled("bmp ", Style::default().fg(theme.muted)),
        Span::styled(format!("{} ", latest.slow_bitmap), Style::default().fg(theme.fg)),
        Span::styled("draw ", Style::default().fg(theme.muted)),
        Span::styled(format!("{}", latest.slow_draw), Style::default().fg(theme.fg)),
    ]));
    f.render_widget(counters, chunks[3]);

    if chunks[4].height >= 2 {
        let history: Vec<u64> = app
            .fps
            .samples
            .iter()
            .map(|s| s.janky_percent.round().max(0.0) as u64)
            .collect();
        let spark = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .title(Span::styled(
                        " jank % history ",
                        Style::default().fg(theme.muted),
                    ))
                    .border_style(Style::default().fg(theme.surface)),
            )
            .data(&history)
            .max(50)
            .bar_set(symbols::bar::NINE_LEVELS)
            .style(Style::default().fg(theme.warn));
        f.render_widget(spark, chunks[4]);
    }
}

fn jank_color(pct: f32, theme: &Theme) -> ratatui::style::Color {
    if pct < 1.0 {
        theme.success
    } else if pct < 5.0 {
        theme.warn
    } else {
        theme.error
    }
}

fn pct_color(ms: f32, theme: &Theme) -> ratatui::style::Color {
    if ms <= 16.0 {
        theme.success
    } else if ms <= 33.0 {
        theme.warn
    } else {
        theme.error
    }
}
