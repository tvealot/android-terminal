use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::panel::{PanelId, PANELS};
use crate::theme::Theme;
use crate::{files_ui, gradle_ui, logcat_ui, monitor_ui, network_ui};

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
}

fn render_header(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut spans = vec![Span::styled(
        " droidscope ",
        Style::default()
            .fg(theme.bg)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )];
    spans.push(Span::raw(" "));
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
    if app.visible.is_empty() {
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

    let logcat_visible = app.visible.contains(&PanelId::Logcat);
    let monitor_visible = app.visible.contains(&PanelId::Monitor);
    let network_visible = app.visible.contains(&PanelId::Network);
    let files_visible = app.visible.contains(&PanelId::Files);
    let gradle_visible = app.visible.contains(&PanelId::Gradle);

    let top_visible = logcat_visible;
    let middle_visible = monitor_visible || network_visible;
    let bottom_visible = files_visible || gradle_visible;

    let mut weights = Vec::new();
    if top_visible {
        weights.push(45);
    }
    if middle_visible {
        weights.push(22);
    }
    if bottom_visible {
        weights.push(33);
    }

    let total: u32 = weights.iter().sum();
    let constraints: Vec<Constraint> = weights
        .iter()
        .map(|weight| Constraint::Ratio(*weight, total))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut index = 0;
    if top_visible {
        render_panel(
            f,
            rows[index],
            PanelId::Logcat,
            app,
            theme,
            app.focus == PanelId::Logcat,
        );
        index += 1;
    }
    if middle_visible {
        render_monitor_section(f, rows[index], app, theme, monitor_visible, network_visible);
        index += 1;
    }
    if bottom_visible {
        render_bottom_section(f, rows[index], app, theme, files_visible, gradle_visible);
    }
}

fn render_monitor_section(
    f: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    monitor_visible: bool,
    network_visible: bool,
) {
    match (monitor_visible, network_visible) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(area);
            render_panel(
                f,
                cols[0],
                PanelId::Monitor,
                app,
                theme,
                app.focus == PanelId::Monitor,
            );
            render_panel(
                f,
                cols[1],
                PanelId::Network,
                app,
                theme,
                app.focus == PanelId::Network,
            );
        }
        (true, false) => render_panel(
            f,
            area,
            PanelId::Monitor,
            app,
            theme,
            app.focus == PanelId::Monitor,
        ),
        (false, true) => render_panel(
            f,
            area,
            PanelId::Network,
            app,
            theme,
            app.focus == PanelId::Network,
        ),
        (false, false) => {}
    }
}

fn render_bottom_section(
    f: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    files_visible: bool,
    gradle_visible: bool,
) {
    match (files_visible, gradle_visible) {
        (true, true) => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_panel(
                f,
                cols[0],
                PanelId::Files,
                app,
                theme,
                app.focus == PanelId::Files,
            );
            render_panel(
                f,
                cols[1],
                PanelId::Gradle,
                app,
                theme,
                app.focus == PanelId::Gradle,
            );
        }
        (true, false) => render_panel(
            f,
            area,
            PanelId::Files,
            app,
            theme,
            app.focus == PanelId::Files,
        ),
        (false, true) => render_panel(
            f,
            area,
            PanelId::Gradle,
            app,
            theme,
            app.focus == PanelId::Gradle,
        ),
        (false, false) => {}
    }
}

fn render_panel(f: &mut Frame, area: Rect, id: PanelId, app: &App, theme: &Theme, focused: bool) {
    match id {
        PanelId::Logcat => logcat_ui::render(f, area, app, theme, focused),
        PanelId::Monitor => monitor_ui::render(f, area, app, theme, focused),
        PanelId::Gradle => gradle_ui::render(f, area, app, theme, focused),
        PanelId::Files => files_ui::render(f, area, app, theme, focused),
        PanelId::Network => network_ui::render(f, area, app, theme, focused),
    }
}

fn render_footer(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let text = if let Some(flash) = &app.status {
        let style = Style::default().fg(if flash.error {
            theme.error
        } else {
            theme.accent
        });
        Line::from(Span::styled(flash.text.clone(), style))
    } else {
        Line::from(vec![
            Span::styled("Alt+1..9 toggle  ", Style::default().fg(theme.muted)),
            Span::styled("letter: focus  ", Style::default().fg(theme.muted)),
            Span::styled("r: run gradle  ", Style::default().fg(theme.muted)),
            Span::styled("tab/files nav  ", Style::default().fg(theme.muted)),
            Span::styled("?: help  ", Style::default().fg(theme.muted)),
            Span::styled("q: quit", Style::default().fg(theme.muted)),
        ])
    };
    f.render_widget(Paragraph::new(text), area);
}

fn render_help(f: &mut Frame, area: Rect, theme: &Theme) {
    let width = area.width.min(64);
    let height = area.height.min(19);
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

    let mut lines = vec![Line::from(Span::styled(
        "Panel control",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    ))];
    for p in PANELS {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  Alt+{}", p.toggle_key),
                Style::default().fg(theme.warn),
            ),
            Span::raw("  toggle  "),
            Span::styled(format!("{}", p.focus_key), Style::default().fg(theme.warn)),
            Span::raw(format!("  focus {}", p.name)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Gradle",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  r  run default task"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Files",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  Enter  expand directory / preview file"));
    lines.push(Line::from("  Tab    switch tree/detail pane"));
    lines.push(Line::from("  <-     collapse directory"));
    lines.push(Line::from("  r      refresh project tree"));
    lines.push(Line::from("  Backspace  close preview"));
    lines.push(Line::from(""));
    lines.push(Line::from("  ?  toggle this help"));
    lines.push(Line::from("  q  quit"));

    f.render_widget(Paragraph::new(lines), inner);
}
