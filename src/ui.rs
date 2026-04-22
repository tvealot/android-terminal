use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::panel::{def, PanelId, PANELS};
use crate::theme::Theme;
use crate::{gradle_ui, issues_ui, logcat_ui, monitor_ui, processes_ui};

pub fn render(f: &mut Frame, app: &App, theme: &Theme) {
    let area = f.area();
    let vchunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);

    render_header(f, vchunks[0], app, theme);
    render_body(f, vchunks[1], app, theme);
    render_footer(f, vchunks[2], app, theme);

    if app.show_help {
        render_help(f, area, theme);
    }

    if let Some(idx) = app.device_selector {
        render_device_selector(f, area, app, theme, idx);
    }
}

fn render_header(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut spans = vec![Span::styled(
        " droidscope ",
        Style::default().fg(theme.bg).bg(theme.accent).add_modifier(Modifier::BOLD),
    )];
    spans.push(Span::raw(" "));
    let dev_label = match app.current_device() {
        Some(s) => format!("[{}]", shorten_serial(&s)),
        None => match app.devices.len() {
            0 => "[no device]".to_string(),
            1 => format!("[{}]", shorten_serial(&app.devices[0].serial)),
            n => format!("[{} devices, d to pick]", n),
        },
    };
    spans.push(Span::styled(
        format!("{} ", dev_label),
        Style::default().fg(theme.muted),
    ));
    for p in PANELS {
        let visible = app.visible.contains(&p.id);
        let focused = app.focus == p.id && visible;
        let (fg, modifier) = if focused {
            (theme.accent, Modifier::BOLD | Modifier::UNDERLINED)
        } else if visible {
            (theme.fg, Modifier::empty())
        } else {
            (theme.muted, Modifier::DIM)
        };
        spans.push(Span::styled(
            format!("[{}] {} ", p.toggle_key, p.name),
            Style::default().fg(fg).add_modifier(modifier),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_body(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let visible = app.visible_ordered();
    if visible.is_empty() {
        let help = Paragraph::new(vec![
            Line::from(Span::styled(
                "All panels are hidden.",
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Press Alt+1..9 to toggle panels, ? for help, q to quit."),
        ])
        .wrap(Wrap { trim: false });
        f.render_widget(help, area);
        return;
    }

    let count = visible.len() as u32;
    let constraints: Vec<Constraint> = visible.iter().map(|_| Constraint::Ratio(1, count)).collect();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (i, id) in visible.iter().enumerate() {
        let focused = app.focus == *id;
        render_panel(f, chunks[i], *id, app, theme, focused);
    }
}

fn render_panel(
    f: &mut Frame,
    area: Rect,
    id: PanelId,
    app: &App,
    theme: &Theme,
    focused: bool,
) {
    match id {
        PanelId::Logcat => logcat_ui::render(f, area, app, theme, focused),
        PanelId::Gradle => gradle_ui::render(f, area, app, theme, focused),
        PanelId::Monitor => monitor_ui::render(f, area, app, theme, focused),
        PanelId::Processes => processes_ui::render(f, area, app, theme, focused),
        PanelId::Issues => issues_ui::render(f, area, app, theme, focused),
        other => render_stub(f, area, other, theme, focused),
    }
}

fn render_stub(f: &mut Frame, area: Rect, id: PanelId, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let d = def(id);
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", d.name),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let msg = Paragraph::new(Line::from(Span::styled(
        "Coming soon",
        Style::default().fg(theme.muted),
    )));
    f.render_widget(msg, inner);
}

fn render_footer(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let text = if app.input_mode == crate::app::InputMode::LogcatFilter {
        Line::from(vec![
            Span::styled(
                "filter: ",
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.logcat.filter.clone(), Style::default().fg(theme.fg)),
            Span::styled("_  ", Style::default().fg(theme.warn)),
            Span::styled("Enter/Esc: exit", Style::default().fg(theme.muted)),
        ])
    } else if app.input_mode == crate::app::InputMode::LogcatPackage {
        Line::from(vec![
            Span::styled(
                "package: ",
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.package_input.clone(), Style::default().fg(theme.fg)),
            Span::styled("_  ", Style::default().fg(theme.warn)),
            Span::styled(
                "Enter: apply  Esc: cancel  (empty = clear)",
                Style::default().fg(theme.muted),
            ),
        ])
    } else if let Some(flash) = &app.status {
        let style = Style::default().fg(if flash.error { theme.error } else { theme.accent });
        Line::from(Span::styled(flash.text.clone(), style))
    } else {
        Line::from(vec![
            Span::styled("Alt+1..7 toggle  ", Style::default().fg(theme.muted)),
            Span::styled("Tab: cycle  ", Style::default().fg(theme.muted)),
            Span::styled("d: device  ", Style::default().fg(theme.muted)),
            Span::styled("/: filter  ", Style::default().fg(theme.muted)),
            Span::styled("P: package  ", Style::default().fg(theme.muted)),
            Span::styled("Space: pause  ", Style::default().fg(theme.muted)),
            Span::styled("r: gradle  ", Style::default().fg(theme.muted)),
            Span::styled("?: help  ", Style::default().fg(theme.muted)),
            Span::styled("q: quit", Style::default().fg(theme.muted)),
        ])
    };
    f.render_widget(Paragraph::new(text), area);
}

fn render_help(f: &mut Frame, area: Rect, theme: &Theme) {
    let width = area.width.min(60);
    let height = area.height.min(16);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let block = Block::default()
        .title(Span::styled(
            " help ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines = vec![
        Line::from(Span::styled(
            "Panel control",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        )),
    ];
    for p in PANELS {
        lines.push(Line::from(vec![
            Span::styled(format!("  Alt+{}", p.toggle_key), Style::default().fg(theme.warn)),
            Span::raw("  toggle  "),
            Span::styled(format!("{}", p.focus_key), Style::default().fg(theme.warn)),
            Span::raw(format!("  focus {}", p.name)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Logcat",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  /  enter filter mode (tag/message substring)"));
    lines.push(Line::from("  L  cycle min level (V→D→I→W→E→V)"));
    lines.push(Line::from("  P  filter by package (pidof)"));
    lines.push(Line::from("  X  clear package filter"));
    lines.push(Line::from("  Space  pause/resume"));
    lines.push(Line::from("  C  clear buffer"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Issues",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  j/k or ↓/↑  navigate"));
    lines.push(Line::from("  C  clear list"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Processes",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  j/k or ↓/↑  navigate"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Gradle",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  r  run default task"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Focus",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  Tab / Shift+Tab  cycle focus across visible panels"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Device",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  d  open device selector"));
    lines.push(Line::from(""));
    lines.push(Line::from("  ?  toggle this help"));
    lines.push(Line::from("  q  quit"));

    f.render_widget(Paragraph::new(lines), inner);
}

fn shorten_serial(s: &str) -> String {
    if s.len() <= 12 {
        s.to_string()
    } else {
        format!("{}…{}", &s[..4], &s[s.len() - 4..])
    }
}

fn render_device_selector(f: &mut Frame, area: Rect, app: &App, theme: &Theme, selected: usize) {
    let width = area.width.min(60);
    let rows = app.devices.len().max(1);
    let height = (rows as u16 + 4).min(area.height);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let block = Block::default()
        .title(Span::styled(
            " select device ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let current = app.current_device();
    let mut lines = Vec::new();
    for (i, d) in app.devices.iter().enumerate() {
        let is_current = Some(&d.serial) == current.as_ref();
        let marker = if is_current { "●" } else { " " };
        let state_color = if d.is_ready() { theme.success } else { theme.warn };
        let row_style = if i == selected {
            Style::default().fg(theme.bg).bg(theme.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", marker), row_style),
            Span::styled(format!("{:<24} ", d.serial), row_style),
            Span::styled(format!("{:<10}", d.state), Style::default().fg(state_color)),
        ]));
    }
    if app.devices.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no devices connected",
            Style::default().fg(theme.muted),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Enter: select   j/k: move   Esc: close",
        Style::default().fg(theme.muted),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}
