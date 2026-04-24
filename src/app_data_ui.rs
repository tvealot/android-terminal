use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};
use ratatui::Frame;

use crate::app::App;
use crate::app_data::{DataEntry, DataEntryKind};
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
            Span::styled(" P", Style::default().fg(theme.warn)),
            Span::styled(" package ", Style::default().fg(theme.muted)),
            Span::styled("───", Style::default().fg(border_color)),
            Span::styled(" r", Style::default().fg(theme.warn)),
            Span::styled("efresh ", Style::default().fg(theme.muted)),
            Span::styled("───", Style::default().fg(border_color)),
            Span::styled(" ↩", Style::default().fg(theme.warn)),
            Span::styled(" open ", Style::default().fg(theme.muted)),
            Span::styled("───", Style::default().fg(border_color)),
            Span::styled(" tab", Style::default().fg(theme.warn)),
            Span::styled(" preview ", Style::default().fg(theme.muted)),
        ]));
    }

    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if app.app_data.preview.is_some() {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
            .split(inner);
        render_list(
            f,
            cols[0],
            app,
            theme,
            focused && !app.app_data.preview_focused,
        );
        render_preview(
            f,
            cols[1],
            app,
            theme,
            focused && app.app_data.preview_focused,
        );
    } else {
        render_list(f, inner, app, theme, focused);
    }
}

fn render_list(f: &mut Frame, area: Rect, app: &App, theme: &Theme, active: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let target = app.target_package.as_deref().unwrap_or("(unset)");
    let header = vec![
        Line::from(vec![
            Span::styled("target  ", Style::default().fg(theme.muted)),
            Span::styled(
                truncate(target, rows[0].width as usize),
                Style::default().fg(theme.accent),
            ),
        ]),
        Line::from(vec![
            Span::styled("path    ", Style::default().fg(theme.muted)),
            Span::styled(
                truncate(&app.app_data.path, rows[0].width as usize),
                Style::default().fg(theme.fg),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(header), rows[0]);

    if app.target_package.is_none() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Press P to set a debuggable package.",
                Style::default().fg(theme.muted),
            ))),
            rows[1],
        );
        return;
    }

    if app.app_data.loading {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "loading via run-as...",
                Style::default().fg(theme.warn),
            ))),
            rows[1],
        );
        return;
    }

    if let Some(error) = &app.app_data.last_error {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                error.clone(),
                Style::default().fg(theme.error),
            )))
            .wrap(Wrap { trim: false }),
            rows[1],
        );
        return;
    }

    if app.app_data.entries.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Press r to list app-private files.",
                Style::default().fg(theme.muted),
            ))),
            rows[1],
        );
        return;
    }

    let visible_height = rows[1].height as usize;
    let selected = app
        .app_data
        .selected
        .min(app.app_data.entries.len().saturating_sub(1));
    let start = if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    };
    let end = (start + visible_height).min(app.app_data.entries.len());
    let items: Vec<ListItem> = app.app_data.entries[start..end]
        .iter()
        .enumerate()
        .map(|(offset, entry)| {
            build_item(
                entry,
                selected == start + offset,
                active,
                theme,
                rows[1].width,
            )
        })
        .collect();
    f.render_widget(List::new(items), rows[1]);

    if app.app_data.entries.len() > visible_height {
        let mut state =
            ScrollbarState::new(app.app_data.entries.len().saturating_sub(visible_height))
                .position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme.muted))
            .track_style(Style::default().fg(theme.surface));
        f.render_stateful_widget(scrollbar, rows[1], &mut state);
    }
}

fn render_preview(f: &mut Frame, area: Rect, app: &App, theme: &Theme, active: bool) {
    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme.surface));
    let inner = separator.inner(area);
    f.render_widget(separator, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let Some(preview) = &app.app_data.preview else {
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);
    let chip_style = if active {
        Style::default()
            .fg(theme.bg)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD)
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {} ", truncate(&preview.path, rows[0].width as usize)),
                chip_style,
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
    let wrapped = wrap_lines(&logical, rows[1].width as usize);
    let visible_height = rows[1].height as usize;
    let scroll = app
        .app_data
        .preview_scroll
        .min(wrapped.len().saturating_sub(visible_height));
    let lines: Vec<Line> = wrapped[scroll..wrapped.len().min(scroll + visible_height)]
        .iter()
        .map(|line| Line::from(Span::styled(line.clone(), Style::default().fg(theme.fg))))
        .collect();
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[1]);
}

fn build_item(
    entry: &DataEntry,
    selected: bool,
    active: bool,
    theme: &Theme,
    width: u16,
) -> ListItem<'static> {
    let style = if selected && active {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else if selected {
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg)
    };
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
