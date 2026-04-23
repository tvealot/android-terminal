use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let block = Block::default()
        .title(Span::styled(
            " gradle ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if !app.jvm_available {
        let msg = Paragraph::new(Line::from(Span::styled(
            "JDK not found. Install Java 17+ and configure gradle.jar_path.",
            Style::default().fg(theme.warn),
        )));
        f.render_widget(msg, inner);
        return;
    }

    let spinner = spinner_char();
    let mut lines: Vec<Line> = Vec::new();

    for task in app.gradle.active.values() {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<8} ", "task"), Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} ", spinner), Style::default().fg(theme.warn)),
            Span::styled(task.path.clone(), Style::default().fg(theme.fg)),
        ]));
    }

    for p in &app.gradle.host_procs {
        let mb = p.rss_kb as f64 / 1024.0;
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<8} ", p.kind),
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("pid {:<6} ", p.pid), Style::default().fg(theme.fg)),
            Span::styled(
                format!("cpu {:>5.1}%  ", p.cpu),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                format!("rss {:>6.0} MB", mb),
                Style::default().fg(theme.muted),
            ),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "no gradle processes",
            Style::default().fg(theme.muted),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn spinner_char() -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let idx = (chrono::Local::now().timestamp_millis() / 100) as usize % FRAMES.len();
    FRAMES[idx]
}
