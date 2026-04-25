use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};
use ratatui::Frame;
use std::rc::Rc;

use crate::app::App;
use crate::app_data::{
    AppDataMode, DataEntry, DataEntryKind, DatabaseEntry, DbTable, PreferenceFile,
    PreferenceFileKind, PreferenceRow,
};
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let mut block = Block::default()
        .title(Span::styled(
            " data ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if focused {
        block = block.title_bottom(Line::from(vec![
            Span::styled(" f", Style::default().fg(theme.warn)),
            Span::styled("iles ", Style::default().fg(theme.muted)),
            Span::styled(" d", Style::default().fg(theme.warn)),
            Span::styled("b ", Style::default().fg(theme.muted)),
            Span::styled(" v", Style::default().fg(theme.warn)),
            Span::styled(" prefs ", Style::default().fg(theme.muted)),
            Span::styled(" P", Style::default().fg(theme.warn)),
            Span::styled(" package ", Style::default().fg(theme.muted)),
            Span::styled(" r", Style::default().fg(theme.warn)),
            Span::styled("efresh ", Style::default().fg(theme.muted)),
            Span::styled(" ↩", Style::default().fg(theme.warn)),
            Span::styled(" open ", Style::default().fg(theme.muted)),
            Span::styled(" tab", Style::default().fg(theme.warn)),
            Span::styled(" detail ", Style::default().fg(theme.muted)),
        ]));
    }

    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    match app.app_data.mode {
        AppDataMode::Files => render_files(f, inner, app, theme, focused),
        AppDataMode::Databases => render_databases(f, inner, app, theme, focused),
        AppDataMode::Preferences => render_preferences(f, inner, app, theme, focused),
    }
}

fn render_files(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    if app.app_data.preview.is_some() {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
            .split(area);
        render_file_list(
            f,
            cols[0],
            app,
            theme,
            focused && !app.app_data.preview_focused,
        );
        render_file_preview(
            f,
            cols[1],
            app,
            theme,
            focused && app.app_data.preview_focused,
        );
    } else {
        render_file_list(f, area, app, theme, focused);
    }
}

fn render_file_list(f: &mut Frame, area: Rect, app: &App, theme: &Theme, active: bool) {
    let rows = render_header(f, area, app, theme, &[("path", &app.app_data.path)]);
    if !render_common_empty(f, rows[1], app, theme, "Press r to list app-private files.") {
        return;
    }

    let visible_height = rows[1].height as usize;
    let selected = app
        .app_data
        .selected
        .min(app.app_data.entries.len().saturating_sub(1));
    let start = visible_start(selected, visible_height);
    let end = (start + visible_height).min(app.app_data.entries.len());
    let items: Vec<ListItem> = app.app_data.entries[start..end]
        .iter()
        .enumerate()
        .map(|(offset, entry)| {
            build_file_item(
                entry,
                selected == start + offset,
                active,
                theme,
                rows[1].width,
            )
        })
        .collect();
    f.render_widget(List::new(items), rows[1]);
    render_scrollbar(
        f,
        rows[1],
        app.app_data.entries.len(),
        visible_height,
        start,
        theme,
    );
}

fn render_file_preview(f: &mut Frame, area: Rect, app: &App, theme: &Theme, active: bool) {
    let inner = render_separator(f, area, theme);
    let Some(preview) = &app.app_data.preview else {
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {} ", truncate(&preview.path, rows[0].width as usize)),
                chip_style(active, theme),
            )),
            Line::from(vec![
                Span::styled(
                    if preview.binary { "binary  " } else { "text  " },
                    Style::default().fg(if preview.binary {
                        theme.warn
                    } else {
                        theme.muted
                    }),
                ),
                Span::styled(
                    if preview.truncated { "truncated" } else { "" },
                    Style::default().fg(theme.warn),
                ),
            ]),
        ]),
        rows[0],
    );

    let logical: Vec<String> = preview.content.lines().map(str::to_string).collect();
    render_wrapped_lines(f, rows[1], &logical, app.app_data.preview_scroll, theme);
}

fn render_databases(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let detail_open = app.app_data.table_preview.is_some();
    if detail_open {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area);
        render_database_list(
            f,
            cols[0],
            app,
            theme,
            focused && !app.app_data.preview_focused,
        );
        render_table_preview(
            f,
            cols[1],
            app,
            theme,
            focused && app.app_data.preview_focused,
        );
    } else {
        render_database_list(f, area, app, theme, focused);
    }
}

fn render_database_list(f: &mut Frame, area: Rect, app: &App, theme: &Theme, active: bool) {
    let database = app
        .app_data
        .current_database
        .as_deref()
        .unwrap_or("(choose database)");
    let rows = render_header(f, area, app, theme, &[("database", database)]);
    if !render_common_empty(f, rows[1], app, theme, "Press r to inspect app databases.") {
        return;
    }

    if app.app_data.current_database.is_some() {
        if app.app_data.tables.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "No user tables or views found.",
                    Style::default().fg(theme.muted),
                ))),
                rows[1],
            );
            return;
        }
        render_simple_list(
            f,
            rows[1],
            &app.app_data.tables,
            app.app_data.table_selected,
            active,
            theme,
            build_table_item,
        );
    } else {
        if app.app_data.databases.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "No files in databases/.",
                    Style::default().fg(theme.muted),
                ))),
                rows[1],
            );
            return;
        }
        render_simple_list(
            f,
            rows[1],
            &app.app_data.databases,
            app.app_data.db_selected,
            active,
            theme,
            build_database_item,
        );
    }
}

fn render_table_preview(f: &mut Frame, area: Rect, app: &App, theme: &Theme, active: bool) {
    let inner = render_separator(f, area, theme);
    let Some(preview) = &app.app_data.table_preview else {
        return;
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {} ", truncate(&preview.table, rows[0].width as usize)),
                chip_style(active, theme),
            )),
            Line::from(Span::styled(
                truncate(&preview.database, rows[0].width as usize),
                Style::default().fg(theme.muted),
            )),
        ]),
        rows[0],
    );
    let mut logical = Vec::new();
    logical.push(format_table_row(&preview.columns));
    for row in &preview.rows {
        logical.push(format_table_row(row));
    }
    if preview.rows.is_empty() {
        logical.push("(no rows)".to_string());
    }
    render_wrapped_lines(f, rows[1], &logical, app.app_data.preview_scroll, theme);
}

fn render_preferences(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let detail_open = app.app_data.preference_preview.is_some();
    if detail_open {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area);
        render_preference_list(
            f,
            cols[0],
            app,
            theme,
            focused && !app.app_data.preview_focused,
        );
        render_preference_preview(
            f,
            cols[1],
            app,
            theme,
            focused && app.app_data.preview_focused,
        );
    } else {
        render_preference_list(f, area, app, theme, focused);
    }
}

fn render_preference_list(f: &mut Frame, area: Rect, app: &App, theme: &Theme, active: bool) {
    let rows = render_header(
        f,
        area,
        app,
        theme,
        &[("source", "shared_prefs + datastore")],
    );
    if !render_common_empty(
        f,
        rows[1],
        app,
        theme,
        "Press r to inspect prefs/datastore.",
    ) {
        return;
    }
    if app.app_data.preference_files.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No shared_prefs/*.xml or files/datastore entries.",
                Style::default().fg(theme.muted),
            ))),
            rows[1],
        );
        return;
    }
    render_simple_list(
        f,
        rows[1],
        &app.app_data.preference_files,
        app.app_data.pref_selected,
        active,
        theme,
        build_preference_file_item,
    );
}

fn render_preference_preview(f: &mut Frame, area: Rect, app: &App, theme: &Theme, active: bool) {
    let inner = render_separator(f, area, theme);
    let Some(preview) = &app.app_data.preference_preview else {
        return;
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);
    let kind = match preview.file.kind {
        PreferenceFileKind::SharedPreferences => "xml",
        PreferenceFileKind::DataStore => "datastore",
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {} ", truncate(&preview.file.name, rows[0].width as usize)),
                chip_style(active, theme),
            )),
            Line::from(Span::styled(kind, Style::default().fg(theme.muted))),
        ]),
        rows[0],
    );
    let mut logical = Vec::new();
    if let Some(message) = &preview.message {
        logical.push(message.clone());
    }
    logical.push(format!("{:<24} {:<10} {}", "key", "type", "value"));
    for row in &preview.rows {
        logical.push(format_preference_row(row));
    }
    if preview.rows.is_empty() {
        logical.push("(no key/value rows)".to_string());
    }
    render_wrapped_lines(f, rows[1], &logical, app.app_data.preview_scroll, theme);
}

fn render_header<'a>(
    f: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    extra: &[(&str, &'a str)],
) -> Rc<[Rect]> {
    let height = 2 + extra.len() as u16;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(height), Constraint::Min(0)])
        .split(area);

    let target = app.target_package.as_deref().unwrap_or("(unset)");
    let mut header = vec![
        Line::from(vec![
            Span::styled("target  ", Style::default().fg(theme.muted)),
            Span::styled(
                truncate(target, rows[0].width as usize),
                Style::default().fg(theme.accent),
            ),
        ]),
        Line::from(vec![
            Span::styled("mode    ", Style::default().fg(theme.muted)),
            Span::styled(app.app_data.mode.label(), Style::default().fg(theme.fg)),
        ]),
    ];
    for (label, value) in extra {
        header.push(Line::from(vec![
            Span::styled(format!("{label:<8}"), Style::default().fg(theme.muted)),
            Span::styled(
                truncate(value, rows[0].width as usize),
                Style::default().fg(theme.fg),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(header), rows[0]);
    rows
}

fn render_common_empty(
    f: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    empty_hint: &str,
) -> bool {
    if app.target_package.is_none() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Press P to set a debuggable package.",
                Style::default().fg(theme.muted),
            ))),
            area,
        );
        return false;
    }

    if app.app_data.loading {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "loading via run-as...",
                Style::default().fg(theme.warn),
            ))),
            area,
        );
        return false;
    }

    if let Some(error) = &app.app_data.last_error {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                error.clone(),
                Style::default().fg(theme.error),
            )))
            .wrap(Wrap { trim: false }),
            area,
        );
        return false;
    }

    let has_content = match app.app_data.mode {
        AppDataMode::Files => !app.app_data.entries.is_empty(),
        AppDataMode::Databases => {
            !app.app_data.databases.is_empty() || !app.app_data.tables.is_empty()
        }
        AppDataMode::Preferences => !app.app_data.preference_files.is_empty(),
    };
    if !has_content {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                empty_hint.to_string(),
                Style::default().fg(theme.muted),
            ))),
            area,
        );
        return false;
    }
    true
}

fn render_simple_list<T>(
    f: &mut Frame,
    area: Rect,
    items: &[T],
    selected: usize,
    active: bool,
    theme: &Theme,
    build: fn(&T, bool, bool, &Theme, u16) -> ListItem<'static>,
) {
    let visible_height = area.height as usize;
    let selected = selected.min(items.len().saturating_sub(1));
    let start = visible_start(selected, visible_height);
    let end = (start + visible_height).min(items.len());
    let rendered: Vec<ListItem> = items[start..end]
        .iter()
        .enumerate()
        .map(|(offset, item)| build(item, selected == start + offset, active, theme, area.width))
        .collect();
    f.render_widget(List::new(rendered), area);
    render_scrollbar(f, area, items.len(), visible_height, start, theme);
}

fn build_file_item(
    entry: &DataEntry,
    selected: bool,
    active: bool,
    theme: &Theme,
    width: u16,
) -> ListItem<'static> {
    let style = item_style(selected, active, theme);
    let marker = if selected && active { ">" } else { " " };
    let kind = match &entry.kind {
        DataEntryKind::Directory => "/",
        DataEntryKind::File => " ",
        DataEntryKind::Other => "?",
    };
    let size = entry
        .size_bytes
        .map(format_size)
        .unwrap_or_else(|| "-".to_string());
    let name_width = (width as usize).saturating_sub(24);
    ListItem::new(Line::from(vec![
        Span::styled(format!("{marker} {kind} "), style),
        Span::styled(
            format!(
                "{:<name_width$}",
                truncate(&entry.name, name_width),
                name_width = name_width
            ),
            style,
        ),
        Span::styled(format!(" {:>8} ", size), Style::default().fg(theme.muted)),
        Span::styled(truncate(&entry.meta, 12), Style::default().fg(theme.muted)),
    ]))
}

fn build_database_item(
    entry: &DatabaseEntry,
    selected: bool,
    active: bool,
    theme: &Theme,
    width: u16,
) -> ListItem<'static> {
    let style = item_style(selected, active, theme);
    let marker = if selected && active { ">" } else { " " };
    let size = entry
        .size_bytes
        .map(format_size)
        .unwrap_or_else(|| "-".to_string());
    let name_width = (width as usize).saturating_sub(14);
    ListItem::new(Line::from(vec![
        Span::styled(format!("{marker} db "), style),
        Span::styled(
            format!(
                "{:<name_width$}",
                truncate(&entry.name, name_width),
                name_width = name_width
            ),
            style,
        ),
        Span::styled(format!(" {:>8}", size), Style::default().fg(theme.muted)),
    ]))
}

fn build_table_item(
    table: &DbTable,
    selected: bool,
    active: bool,
    theme: &Theme,
    width: u16,
) -> ListItem<'static> {
    let style = item_style(selected, active, theme);
    let marker = if selected && active { ">" } else { " " };
    let name_width = (width as usize).saturating_sub(12);
    ListItem::new(Line::from(vec![
        Span::styled(format!("{marker} {} ", truncate(&table.kind, 5)), style),
        Span::styled(
            format!(
                "{:<name_width$}",
                truncate(&table.name, name_width),
                name_width = name_width
            ),
            style,
        ),
    ]))
}

fn build_preference_file_item(
    file: &PreferenceFile,
    selected: bool,
    active: bool,
    theme: &Theme,
    width: u16,
) -> ListItem<'static> {
    let style = item_style(selected, active, theme);
    let marker = if selected && active { ">" } else { " " };
    let kind = match file.kind {
        PreferenceFileKind::SharedPreferences => "xml",
        PreferenceFileKind::DataStore => "ds",
    };
    let size = file
        .size_bytes
        .map(format_size)
        .unwrap_or_else(|| "-".to_string());
    let name_width = (width as usize).saturating_sub(15);
    ListItem::new(Line::from(vec![
        Span::styled(format!("{marker} {kind} "), style),
        Span::styled(
            format!(
                "{:<name_width$}",
                truncate(&file.name, name_width),
                name_width = name_width
            ),
            style,
        ),
        Span::styled(format!(" {:>8}", size), Style::default().fg(theme.muted)),
    ]))
}

fn format_table_row(row: &[String]) -> String {
    row.iter()
        .map(|cell| cell.replace('\n', " "))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn format_preference_row(row: &PreferenceRow) -> String {
    format!(
        "{:<24} {:<10} {}",
        truncate(&row.key, 24),
        truncate(&row.value_type, 10),
        row.value.replace('\n', " ")
    )
}

fn render_separator(f: &mut Frame, area: Rect, theme: &Theme) -> Rect {
    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme.surface));
    let inner = separator.inner(area);
    f.render_widget(separator, area);
    inner
}

fn chip_style(active: bool, theme: &Theme) -> Style {
    if active {
        Style::default()
            .fg(theme.bg)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD)
    }
}

fn item_style(selected: bool, active: bool, theme: &Theme) -> Style {
    if selected && active {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else if selected {
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg)
    }
}

fn render_wrapped_lines(f: &mut Frame, area: Rect, lines: &[String], scroll: usize, theme: &Theme) {
    let wrapped = wrap_lines(lines, area.width as usize);
    let visible_height = area.height as usize;
    let scroll = scroll.min(wrapped.len().saturating_sub(visible_height));
    let visible: Vec<Line> = wrapped[scroll..wrapped.len().min(scroll + visible_height)]
        .iter()
        .map(|line| Line::from(Span::styled(line.clone(), Style::default().fg(theme.fg))))
        .collect();
    f.render_widget(Paragraph::new(visible).wrap(Wrap { trim: false }), area);
    render_scrollbar(f, area, wrapped.len(), visible_height, scroll, theme);
}

fn render_scrollbar(
    f: &mut Frame,
    area: Rect,
    len: usize,
    visible_height: usize,
    start: usize,
    theme: &Theme,
) {
    if len > visible_height {
        let mut state = ScrollbarState::new(len.saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme.muted))
            .track_style(Style::default().fg(theme.surface));
        f.render_stateful_widget(scrollbar, area, &mut state);
    }
}

fn visible_start(selected: usize, visible_height: usize) -> usize {
    if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    }
}

fn wrap_lines(lines: &[String], width: usize) -> Vec<String> {
    if width == 0 {
        return lines.to_vec();
    }

    let mut wrapped = Vec::new();
    for line in lines {
        if line.is_empty() {
            wrapped.push(String::new());
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut start = 0;
        while start < chars.len() {
            let end = (start + width).min(chars.len());
            wrapped.push(chars[start..end].iter().collect());
            start = end;
        }
    }
    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    wrapped
}

fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1}K", size as f64 / 1024.0)
    } else {
        format!("{:.1}M", size as f64 / (1024.0 * 1024.0))
    }
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{head}…")
    }
}
