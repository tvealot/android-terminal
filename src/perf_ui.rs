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
    let pkg = app.perf.current_package();
    let title = match (&pkg, app.perf.latest()) {
        (Some(p), Some(s)) => format!(" perf · {} (pid {}) ", p, s.pid),
        (Some(p), None) => format!(" perf · {} ", p),
        (None, _) => " perf ".to_string(),
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

    let Some(latest) = app.perf.latest() else {
        let msg = if app.perf.last_error.is_some() {
            "perf error — check adb / package running"
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
            Constraint::Length(1), // pss / rss / cpu
            Constraint::Length(1), // cpu gauge
            Constraint::Length(1), // app summary a
            Constraint::Length(1), // app summary b
            Constraint::Length(1), // heap alloc + gc
            Constraint::Length(1), // jank
            Constraint::Min(1),    // sparkline
        ])
        .split(inner);

    let top = Paragraph::new(Line::from(vec![
        Span::styled("pss ", Style::default().fg(theme.accent)),
        Span::styled(
            format!("{} ", fmt_mb(latest.pss_total_kb)),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
        Span::styled("rss ", Style::default().fg(theme.accent)),
        Span::styled(
            format!("{} ", fmt_mb(latest.rss_total_kb)),
            Style::default().fg(theme.fg),
        ),
        Span::styled("cpu ", Style::default().fg(theme.accent)),
        Span::styled(
            format!("{:.1}%", latest.cpu_percent),
            Style::default()
                .fg(cpu_color(latest.cpu_percent, theme))
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    f.render_widget(top, chunks[0]);

    let cpu_gauge = Gauge::default()
        .gauge_style(Style::default().fg(cpu_color(latest.cpu_percent, theme)))
        .ratio((latest.cpu_percent as f64 / 100.0).clamp(0.0, 1.0))
        .label(Span::styled(
            format!("cpu {:.1}%", latest.cpu_percent),
            Style::default().fg(theme.bg),
        ));
    f.render_widget(cpu_gauge, chunks[1]);

    let row_a = Paragraph::new(Line::from(vec![
        Span::styled("java ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}  ", fmt_mb(latest.java_heap_kb)),
            Style::default().fg(theme.fg),
        ),
        Span::styled("native ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}  ", fmt_mb(latest.native_heap_kb)),
            Style::default().fg(theme.fg),
        ),
        Span::styled("gfx ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}", fmt_mb(latest.graphics_kb)),
            Style::default().fg(theme.fg),
        ),
    ]));
    f.render_widget(row_a, chunks[2]);

    let row_b = Paragraph::new(Line::from(vec![
        Span::styled("code ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}  ", fmt_mb(latest.code_kb)),
            Style::default().fg(theme.fg),
        ),
        Span::styled("stack ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}  ", fmt_mb(latest.stack_kb)),
            Style::default().fg(theme.fg),
        ),
        Span::styled("other ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}  ", fmt_mb(latest.private_other_kb)),
            Style::default().fg(theme.fg),
        ),
        Span::styled("sys ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{}", fmt_mb(latest.system_kb)),
            Style::default().fg(theme.fg),
        ),
    ]));
    f.render_widget(row_b, chunks[3]);

    let gc_color = if latest.gc_delta > 0 { theme.warn } else { theme.muted };
    let row_c = Paragraph::new(Line::from(vec![
        Span::styled("heap ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} alloc  ", fmt_mb(latest.dalvik_heap_alloc_kb)),
            Style::default().fg(theme.fg),
        ),
        Span::styled("native ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} alloc  ", fmt_mb(latest.native_heap_alloc_kb)),
            Style::default().fg(theme.fg),
        ),
        Span::styled("gc ", Style::default().fg(theme.muted)),
        Span::styled(
            format!(
                "{}{}",
                latest.gc_markers,
                if latest.gc_delta > 0 { " •" } else { "" }
            ),
            Style::default().fg(gc_color).add_modifier(Modifier::BOLD),
        ),
    ]));
    f.render_widget(row_c, chunks[4]);

    let jank = Paragraph::new(Line::from(vec![
        Span::styled("jank ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{:.2}% ({} fr)  ", latest.jank_percent, latest.frames_total),
            Style::default()
                .fg(jank_color(latest.jank_percent, theme))
                .add_modifier(Modifier::BOLD),
        ),
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
    f.render_widget(jank, chunks[5]);

    if chunks[6].height >= 2 {
        let history: Vec<u64> = app
            .perf
            .samples
            .iter()
            .map(|s| (s.pss_total_kb / 1024).max(0))
            .collect();
        let max = history.iter().copied().max().unwrap_or(1).max(1);
        let spark = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .title(Span::styled(
                        format!(" pss MB history (max {} MB) ", max),
                        Style::default().fg(theme.muted),
                    ))
                    .border_style(Style::default().fg(theme.surface)),
            )
            .data(&history)
            .max(max)
            .bar_set(symbols::bar::NINE_LEVELS)
            .style(Style::default().fg(theme.accent));
        f.render_widget(spark, chunks[6]);
    }
}

fn fmt_mb(kb: u64) -> String {
    if kb >= 10 * 1024 {
        format!("{} MB", kb / 1024)
    } else if kb >= 1024 {
        format!("{:.1} MB", kb as f32 / 1024.0)
    } else {
        format!("{} KB", kb)
    }
}

fn cpu_color(pct: f32, theme: &Theme) -> ratatui::style::Color {
    if pct < 30.0 {
        theme.success
    } else if pct < 70.0 {
        theme.warn
    } else {
        theme.error
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

