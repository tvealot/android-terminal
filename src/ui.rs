use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::layout::{cell_rect, LayoutEditor, LayoutGrid};
use crate::panel::{def, PanelId, PANELS};
use crate::theme::Theme;
use crate::{
    app_control_ui, app_data_ui, devices_ui, files_ui, fps_ui, gradle_ui, intents_ui, issues_ui,
    logcat_ui, monitor_ui, network_ui, processes_ui, shell_ui,
};

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
    if let Some(editor) = &app.layout_editor {
        render_layout_editor(f, vchunks[1], editor, theme);
    } else {
        render_body(f, vchunks[1], app, theme);
    }
    render_footer(f, vchunks[2], app, theme);

    if let Some(id) = app.zoom {
        render_zoom(f, vchunks[1], id, app, theme);
    }

    if app.show_help {
        render_help(f, area, theme);
    }

    if let Some(idx) = app.device_selector {
        render_device_selector(f, area, app, theme, idx);
    }

    if app.project_picker.is_some() {
        render_project_picker(f, area, app, theme);
    }

    if app.emulator_picker.is_some() {
        render_emulator_picker(f, area, app, theme);
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
    spans.push(Span::styled(
        format!("[screen {}] ", app.screen_label()),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
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
    if let Some(grid) = &app.layout {
        render_grid_body(f, area, grid, app, theme);
        return;
    }

    let visible = app.visible_ordered();
    if visible.is_empty() {
        let help = Paragraph::new(vec![
            Line::from(Span::styled(
                "All panels are hidden.",
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Press panel toggle keys, [/] for screens, 0 for layout editor, ? for help, q to quit."),
        ])
        .wrap(Wrap { trim: false });
        f.render_widget(help, area);
        return;
    }

    let count = visible.len() as u32;
    let constraints: Vec<Constraint> = visible
        .iter()
        .map(|_| Constraint::Ratio(1, count))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (i, id) in visible.iter().enumerate() {
        let focused = app.focus == *id;
        let row = rows[i];
        if *id == PanelId::Monitor {
            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Ratio(2, 3), Constraint::Ratio(1, 3)])
                .split(row);
            render_panel(f, split[1], *id, app, theme, focused);
        } else {
            render_panel(f, row, *id, app, theme, focused);
        }
    }
}

fn render_grid_body(f: &mut Frame, area: Rect, grid: &LayoutGrid, app: &App, theme: &Theme) {
    if grid.cells.is_empty() || grid.cols == 0 || grid.rows == 0 {
        return;
    }
    for cell in &grid.cells {
        let rect = cell_rect(area, grid, cell.x, cell.y, cell.w, cell.h);
        if rect.width == 0 || rect.height == 0 {
            continue;
        }
        let focused = app.focus == cell.panel;
        render_panel(f, rect, cell.panel, app, theme, focused);
    }
}

fn render_layout_editor(f: &mut Frame, area: Rect, editor: &LayoutEditor, theme: &Theme) {
    let outer = Block::default()
        .title(Span::styled(
            " layout editor ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let help_h: u16 = 6;
    let grid_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height.saturating_sub(help_h),
    };
    let help_area = Rect {
        x: inner.x,
        y: inner.y + grid_area.height,
        width: inner.width,
        height: inner.height.saturating_sub(grid_area.height),
    };

    let grid = &editor.grid;
    if grid.cols > 0 && grid.rows > 0 && grid_area.width > 0 && grid_area.height > 0 {
        for cell in &grid.cells {
            let r = cell_rect(grid_area, grid, cell.x, cell.y, cell.w, cell.h);
            if r.width == 0 || r.height == 0 {
                continue;
            }
            let name = def(cell.panel).name;
            let block = Block::default()
                .title(Span::styled(
                    format!(" {} ", name),
                    Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.muted));
            f.render_widget(block, r);
        }

        let (sx, sy, sw, sh) = editor.selection_rect();
        let sel = cell_rect(grid_area, grid, sx, sy, sw, sh);
        if sel.width > 0 && sel.height > 0 {
            let sel_style = if editor.sel_start.is_some() {
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            };
            let sel_block = Block::default()
                .borders(Borders::ALL)
                .border_style(sel_style);
            f.render_widget(Clear, sel);
            f.render_widget(sel_block, sel);
        }
    }

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            format!("grid {}x{}  ", grid.cols, grid.rows),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("cur ({},{})  ", editor.cursor_x, editor.cursor_y),
            Style::default().fg(theme.fg),
        ),
        Span::styled(
            format!("cells {}", grid.cells.len()),
            Style::default().fg(theme.muted),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("h/j/k/l ", Style::default().fg(theme.warn)),
        Span::raw("move  "),
        Span::styled("v ", Style::default().fg(theme.warn)),
        Span::raw("toggle selection  "),
        Span::styled("1..9/A/B/U/F ", Style::default().fg(theme.warn)),
        Span::raw("assign panel"),
    ]));
    lines.push(Line::from(vec![
        Span::styled("x ", Style::default().fg(theme.warn)),
        Span::raw("delete cell  "),
        Span::styled("c ", Style::default().fg(theme.warn)),
        Span::raw("clear all  "),
        Span::styled("[ ] ", Style::default().fg(theme.warn)),
        Span::raw("cols -/+  "),
        Span::styled("- = ", Style::default().fg(theme.warn)),
        Span::raw("rows -/+"),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Enter ", Style::default().fg(theme.success)),
        Span::raw("save  "),
        Span::styled("Esc ", Style::default().fg(theme.error)),
        Span::raw("cancel"),
    ]));
    if let Some(m) = &editor.message {
        lines.push(Line::from(Span::styled(
            m.clone(),
            Style::default().fg(theme.success),
        )));
    }
    f.render_widget(Paragraph::new(lines), help_area);
}

fn render_panel(f: &mut Frame, area: Rect, id: PanelId, app: &App, theme: &Theme, focused: bool) {
    match id {
        PanelId::Logcat => logcat_ui::render(f, area, app, theme, focused),
        PanelId::Gradle => gradle_ui::render(f, area, app, theme, focused),
        PanelId::Monitor => monitor_ui::render(f, area, app, theme, focused),
        PanelId::Processes => processes_ui::render(f, area, app, theme, focused),
        PanelId::Issues => issues_ui::render(f, area, app, theme, focused),
        PanelId::Files => files_ui::render(f, area, app, theme, focused),
        PanelId::Network => network_ui::render(f, area, app, theme, focused),
        PanelId::Devices => devices_ui::render(f, area, app, theme, focused),
        PanelId::Shell => shell_ui::render(f, area, app, theme, focused),
        PanelId::AppControl => app_control_ui::render(f, area, app, theme, focused),
        PanelId::AppData => app_data_ui::render(f, area, app, theme, focused),
        PanelId::Intents => intents_ui::render(f, area, app, theme, focused),
        PanelId::Fps => fps_ui::render(f, area, app, theme, focused),
    }
}

fn render_footer(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let text = if app.input_mode == crate::app::InputMode::LogcatFilter {
        Line::from(vec![
            Span::styled(
                "filter: ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.logcat.filter.clone(), Style::default().fg(theme.fg)),
            Span::styled("_  ", Style::default().fg(theme.warn)),
            Span::styled("Enter/Esc: exit", Style::default().fg(theme.muted)),
        ])
    } else if app.input_mode == crate::app::InputMode::LogcatPackage {
        Line::from(vec![
            Span::styled(
                "package: ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.package_input.clone(), Style::default().fg(theme.fg)),
            Span::styled("_  ", Style::default().fg(theme.warn)),
            Span::styled(
                "Enter: apply  Esc: cancel  (empty = clear)",
                Style::default().fg(theme.muted),
            ),
        ])
    } else if app.input_mode == crate::app::InputMode::FpsPackage {
        Line::from(vec![
            Span::styled(
                "fps package: ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.fps_package_input.clone(), Style::default().fg(theme.fg)),
            Span::styled("_  ", Style::default().fg(theme.warn)),
            Span::styled(
                "Enter: apply  Esc: cancel  (empty = clear)",
                Style::default().fg(theme.muted),
            ),
        ])
    } else if app.input_mode == crate::app::InputMode::TargetPackage {
        Line::from(vec![
            Span::styled(
                "target package: ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.target_package_input.clone(),
                Style::default().fg(theme.fg),
            ),
            Span::styled("_  ", Style::default().fg(theme.warn)),
            Span::styled(
                "Enter: apply  Esc: cancel  (empty = clear)",
                Style::default().fg(theme.muted),
            ),
        ])
    } else if app.input_mode == crate::app::InputMode::DeepLinkUrl {
        Line::from(vec![
            Span::styled(
                "deep link: ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.deep_link_input.clone(), Style::default().fg(theme.fg)),
            Span::styled("_  ", Style::default().fg(theme.warn)),
            Span::styled(
                "Enter: apply  Esc: cancel",
                Style::default().fg(theme.muted),
            ),
        ])
    } else if let Some(flash) = &app.status {
        let style = Style::default().fg(if flash.error {
            theme.error
        } else {
            theme.accent
        });
        Line::from(Span::styled(flash.text.clone(), style))
    } else {
        Line::from(vec![
            Span::styled("panel keys toggle  ", Style::default().fg(theme.muted)),
            Span::styled("[/] screens  ", Style::default().fg(theme.muted)),
            Span::styled("0 layout  ", Style::default().fg(theme.muted)),
            Span::styled("Tab: cycle  ", Style::default().fg(theme.muted)),
            Span::styled("d: device  ", Style::default().fg(theme.muted)),
            Span::styled("w: project  ", Style::default().fg(theme.muted)),
            Span::styled("e: emulator  ", Style::default().fg(theme.muted)),
            Span::styled("A: app  ", Style::default().fg(theme.muted)),
            Span::styled("B: data  ", Style::default().fg(theme.muted)),
            Span::styled("U: intents  ", Style::default().fg(theme.muted)),
            Span::styled("F: fps  ", Style::default().fg(theme.muted)),
            Span::styled("z: zoom  ", Style::default().fg(theme.muted)),
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
    let height = area.height.min(56);
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
                format!("  {}", p.toggle_key),
                Style::default().fg(theme.warn),
            ),
            Span::raw("  toggle  "),
            Span::styled(format!("{}", p.focus_key), Style::default().fg(theme.warn)),
            Span::raw(format!("  focus {}", p.name)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Logcat",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  /  enter filter mode (tag/message substring)"));
    lines.push(Line::from("  R  toggle regex filter"));
    lines.push(Line::from("  L  cycle min level (V→D→I→W→E→V)"));
    lines.push(Line::from("  P  filter by package (pidof)"));
    lines.push(Line::from("  X  clear package filter"));
    lines.push(Line::from("  Space  pause/resume"));
    lines.push(Line::from("  C  clear buffer"));
    lines.push(Line::from("  j/k ↑/↓  scroll 1 line   PgUp/PgDn  20"));
    lines.push(Line::from("  gg / G  jump top / bottom (follow tail)"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Issues",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  j/k or ↓/↑  navigate (or scroll detail)"));
    lines.push(Line::from("  Enter  open/close stacktrace detail"));
    lines.push(Line::from("  y  copy full stacktrace of selected issue"));
    lines.push(Line::from("  C  clear list"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Processes",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  j/k or ↓/↑  navigate"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Gradle",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  r  run default task"));
    lines.push(Line::from("  j/k  navigate host processes"));
    lines.push(Line::from("  K  SIGTERM selected process"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Files",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  j/k or ↓/↑  navigate tree"));
    lines.push(Line::from("  Enter/→  expand dir or open file"));
    lines.push(Line::from("  ←/Backspace  collapse / close detail"));
    lines.push(Line::from("  Tab  switch tree ↔ detail (when open)"));
    lines.push(Line::from("  r  refresh"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "App",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  A / a  toggle/focus app control"));
    lines.push(Line::from("  P  set target package"));
    lines.push(Line::from("  j/k  choose action   Enter run"));
    lines.push(Line::from("  !  confirm destructive action"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Data",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  B / b  toggle/focus app data browser"));
    lines.push(Line::from("  P  set target package   r refresh"));
    lines.push(Line::from(
        "  Enter open dir/file   ←/Backspace close or parent",
    ));
    lines.push(Line::from("  Tab  switch to preview pane"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Intents",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  U / u  toggle/focus intent runner"));
    lines.push(Line::from("  /  edit deep link URL   Enter launch"));
    lines.push(Line::from(
        "  P  set target package   T target/resolver mode",
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Shell",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  s / 9  focus → auto `adb shell`"));
    lines.push(Line::from("  Ctrl+\\  defocus (cycle to next panel)"));
    lines.push(Line::from("  All keys route to the PTY while focused"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Focus",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(
        "  Tab / Shift+Tab  cycle focus across visible panels",
    ));
    lines.push(Line::from("  [ / ]  previous / next layout screen"));
    lines.push(Line::from("  z  zoom focused panel (Esc to close)"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Device",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("  d  open device selector"));
    lines.push(Line::from("  8 / v  devices panel (j/k + Enter to switch)"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Project",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(
        "  w  pick Android project (scans ~/Documents, sorted by mtime)",
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Layout",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(
        "  [ / ]  switch screens; each screen keeps its own panels/focus/layout",
    ));
    lines.push(Line::from("  0  open grid layout editor"));
    lines.push(Line::from(
        "  In editor: h/j/k/l move  v select  1..9/A/B/U/F assign",
    ));
    lines.push(Line::from("  x delete  c clear  [ ] cols  - = rows"));
    lines.push(Line::from("  Enter save  Esc cancel"));
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

fn render_project_picker(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(picker) = &app.project_picker else {
        return;
    };
    let width = area.width.min(80);
    let rows_needed = picker.entries.len().max(1) as u16 + 5;
    let height = rows_needed.min(area.height);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let title = format!(" select project  ({}) ", picker.root.display());
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let current = app.config.gradle.project_dir.as_ref();
    let mut lines = Vec::new();
    if picker.loading {
        lines.push(Line::from(Span::styled(
            "  scanning…",
            Style::default().fg(theme.muted),
        )));
    } else if picker.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no Android projects (gradlew) found",
            Style::default().fg(theme.muted),
        )));
    } else {
        let name_w: usize = 28;
        let date_w: usize = 16;
        for (i, e) in picker.entries.iter().enumerate() {
            let is_current = current.map(|c| c == &e.path).unwrap_or(false);
            let marker = if is_current { "●" } else { " " };
            let name = e
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| e.display.clone());
            let row_style = if i == picker.selected {
                Style::default()
                    .fg(theme.bg)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };
            let path_style = if i == picker.selected {
                Style::default().fg(theme.bg).bg(theme.accent)
            } else {
                Style::default().fg(theme.muted)
            };
            let date_style = if i == picker.selected {
                Style::default().fg(theme.bg).bg(theme.accent)
            } else {
                Style::default().fg(theme.warn)
            };
            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", marker), row_style),
                Span::styled(
                    format!("{:<width$}", truncate(&name, name_w), width = name_w),
                    row_style,
                ),
                Span::styled(
                    format!(" {:<width$} ", e.modified_label(), width = date_w),
                    date_style,
                ),
                Span::styled(truncate(&e.display, 100), path_style),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Enter: select   j/k: move   Esc: close",
        Style::default().fg(theme.muted),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_zoom(f: &mut Frame, area: Rect, id: PanelId, app: &App, theme: &Theme) {
    let width = (area.width as u32 * 9 / 10) as u16;
    let height = (area.height as u32 * 9 / 10) as u16;
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    render_panel(f, rect, id, app, theme, true);
    let hint = Rect {
        x: rect.x + 1,
        y: rect.y + rect.height.saturating_sub(1),
        width: rect.width.saturating_sub(2),
        height: 1,
    };
    if hint.height > 0 {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Esc / z: close zoom",
                Style::default().fg(theme.muted),
            ))),
            hint,
        );
    }
}

fn render_emulator_picker(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(picker) = &app.emulator_picker else {
        return;
    };
    let width = area.width.min(60);
    let rows_needed = picker.entries.len().max(1) as u16 + 5;
    let height = rows_needed.min(area.height);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let block = Block::default()
        .title(Span::styled(
            " launch emulator ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines: Vec<Line> = Vec::new();
    if picker.loading {
        lines.push(Line::from(Span::styled(
            "  scanning AVDs…",
            Style::default().fg(theme.muted),
        )));
    } else if picker.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no AVDs found (check `emulator -list-avds`)",
            Style::default().fg(theme.muted),
        )));
    } else {
        for (i, name) in picker.entries.iter().enumerate() {
            let row_style = if i == picker.selected {
                Style::default()
                    .fg(theme.bg)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };
            lines.push(Line::from(vec![
                Span::styled("  ", row_style),
                Span::styled(truncate(name, 56), row_style),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Enter: launch   j/k: move   Esc: close",
        Style::default().fg(theme.muted),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", head)
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
        let state_color = if d.is_ready() {
            theme.success
        } else {
            theme.warn
        };
        let row_style = if i == selected {
            Style::default()
                .fg(theme.bg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
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
