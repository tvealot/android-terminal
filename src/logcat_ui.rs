use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::logcat::{LogLevel, LogcatState};
use crate::theme::{hashed_color, Theme};

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let level = app.logcat.min_level;
    let level_part = if level == LogLevel::Verbose {
        String::new()
    } else {
        format!(" [{}+]", level.short())
    };
    let pkg_part = match (&app.logcat.filter_package, app.logcat.filter_pid) {
        (Some(p), Some(pid)) => format!(" pkg={} pid={}", p, pid),
        _ => String::new(),
    };
    let paused_part = if app.logcat.paused { " [PAUSED]" } else { "" };
    let scroll_part = if app.logcat.scroll > 0 { " [SCROLL]" } else { "" };
    let regex_tag = if app.logcat.use_regex { "re:" } else { "" };
    let regex_err = if let Some(e) = &app.logcat.regex_error {
        format!(" !regex:{}", truncate(e, 30))
    } else {
        String::new()
    };
    let filter_hint = if app.input_mode == crate::app::InputMode::LogcatFilter {
        format!(
            " logcat{}{}{}{} — filter: {}{}_{} ",
            level_part, pkg_part, paused_part, scroll_part, regex_tag, app.logcat.filter, regex_err
        )
    } else if !app.logcat.filter.is_empty() {
        format!(
            " logcat{}{}{}{} — filter: {}{}{} ",
            level_part, pkg_part, paused_part, scroll_part, regex_tag, app.logcat.filter, regex_err
        )
    } else {
        format!(" logcat{}{}{}{} ", level_part, pkg_part, paused_part, scroll_part)
    };
    let block = Block::default()
        .title(Span::styled(
            filter_hint,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let height = inner.height as usize;
    let collected: Vec<&crate::logcat::LogLine> = app.logcat.visible();
    let total = collected.len();
    // scroll = lines offset from the bottom. scroll=0 → tail.
    let max_scroll = total.saturating_sub(height);
    let offset = app.logcat.scroll.min(max_scroll);
    let end = total.saturating_sub(offset);
    let start = end.saturating_sub(height);
    let lines: Vec<Line> = collected[start..end]
        .iter()
        .map(|line| render_line(line, &app.logcat, theme))
        .collect();

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}

fn render_line<'a>(line: &'a crate::logcat::LogLine, state: &LogcatState, theme: &Theme) -> Line<'a> {
    let (level_str, level_color) = match line.level {
        LogLevel::Verbose => ("V", theme.muted),
        LogLevel::Debug => ("D", Color::Cyan),
        LogLevel::Info => ("I", theme.success),
        LogLevel::Warn => ("W", theme.warn),
        LogLevel::Error => ("E", theme.error),
        LogLevel::Fatal => ("F", theme.error),
    };
    let mut spans = vec![
        Span::styled(
            format!("{} ", &line.timestamp),
            Style::default().fg(theme.muted),
        ),
        Span::styled(
            format!("{} ", level_str),
            Style::default().fg(level_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<20} ", truncate(&line.tag, 20)),
            Style::default().fg(hashed_color(line.tag.trim(), theme)),
        ),
    ];
    spans.extend(highlight(&line.message, state, theme));
    Line::from(spans)
}

fn highlight<'a>(text: &'a str, state: &LogcatState, theme: &Theme) -> Vec<Span<'a>> {
    let base = Style::default().fg(theme.fg);
    let hit = Style::default()
        .fg(Color::Black)
        .bg(theme.warn)
        .add_modifier(Modifier::BOLD);
    let spans_ranges = state.match_spans(text);
    if spans_ranges.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let mut out = Vec::new();
    let mut cursor = 0;
    for (s, e) in spans_ranges {
        if s < cursor {
            continue;
        }
        if s > cursor {
            out.push(Span::styled(text[cursor..s].to_string(), base));
        }
        out.push(Span::styled(text[s..e].to_string(), hit));
        cursor = e;
    }
    if cursor < text.len() {
        out.push(Span::styled(text[cursor..].to_string(), base));
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut)
    }
}
