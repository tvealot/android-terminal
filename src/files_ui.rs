use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};
use ratatui::Frame;

use crate::app::App;
use crate::files::{DetailKind, FlatEntry};
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let mut block = Block::default()
        .title(Span::styled(
            " files ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if focused {
        block = block.title_bottom(files_action_bar(app, theme, border_color));
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.files.detail_open {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(inner);
        render_tree_pane(
            f,
            cols[0],
            app,
            theme,
            focused && !app.files.detail_focused,
            true,
        );
        render_detail_pane(f, cols[1], app, theme, focused && app.files.detail_focused);
    } else {
        render_tree_pane(f, inner, app, theme, focused, false);
    }
}

fn files_action_bar(
    app: &App,
    theme: &Theme,
    border_color: ratatui::style::Color,
) -> Line<'static> {
    let accent = Style::default().fg(theme.warn);
    let muted = Style::default().fg(theme.muted);
    let border = Style::default().fg(border_color);

    let spans = if app.files.detail_open && app.files.detail_focused {
        vec![
            Span::styled(" jk", accent),
            Span::styled(" scroll ", muted),
            Span::styled("───", border),
            Span::styled(" tab", accent),
            Span::styled(" tree ", muted),
            Span::styled("───", border),
            Span::styled(" backspace", accent),
            Span::styled(" close ", muted),
        ]
    } else if app.files.detail_open {
        vec![
            Span::styled(" ↩", accent),
            Span::styled(" open ", muted),
            Span::styled("───", border),
            Span::styled(" tab", accent),
            Span::styled(" pane ", muted),
            Span::styled("───", border),
            Span::styled(" <-", accent),
            Span::styled(" collapse ", muted),
            Span::styled("───", border),
            Span::styled(" r", accent),
            Span::styled("efresh ", muted),
        ]
    } else {
        vec![
            Span::styled(" ↩", accent),
            Span::styled(" expand/open ", muted),
            Span::styled("───", border),
            Span::styled(" <-", accent),
            Span::styled(" collapse ", muted),
            Span::styled("───", border),
            Span::styled(" r", accent),
            Span::styled("efresh ", muted),
        ]
    };
    Line::from(spans)
}

fn render_tree_pane(
    f: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    active: bool,
    split: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let (label_area, body_area) = if split {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        (parts[0], parts[1])
    } else {
        (Rect::default(), area)
    };

    if split {
        render_pane_chip(f, label_area, "project", active, theme);
    }

    let root_line = app
        .files
        .root_label()
        .map(|label| compact_text(&label, body_area.width as usize))
        .unwrap_or_else(|| "set [gradle].project_dir in config.toml".to_string());

    if body_area.height == 0 {
        return;
    }

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            root_line,
            Style::default().fg(theme.muted),
        ))),
        Rect {
            x: body_area.x,
            y: body_area.y,
            width: body_area.width,
            height: 1,
        },
    );

    let list_area = Rect {
        x: body_area.x,
        y: body_area.y + 1,
        width: body_area.width,
        height: body_area.height.saturating_sub(1),
    };
    if list_area.height == 0 {
        return;
    }

    if let Some(error) = &app.files.error {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                error.clone(),
                Style::default().fg(theme.error),
            ))),
            list_area,
        );
        return;
    }

    let flat = app.files.flatten_visible();
    if app.files.root.is_none() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Point [gradle].project_dir at an Android project.",
                Style::default().fg(theme.muted),
            ))),
            list_area,
        );
        return;
    }
    if flat.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "empty directory",
                Style::default().fg(theme.muted),
            ))),
            list_area,
        );
        return;
    }

    let visible_height = list_area.height as usize;
    let selected = app.files.selected_index.min(flat.len().saturating_sub(1));
    let start = if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    };
    let end = (start + visible_height).min(flat.len());

    let items: Vec<ListItem> = flat[start..end]
        .iter()
        .enumerate()
        .map(|(offset, entry)| build_tree_item(entry, selected == start + offset, active, theme))
        .collect();

    f.render_widget(List::new(items), list_area);

    if flat.len() > visible_height {
        let mut state =
            ScrollbarState::new(flat.len().saturating_sub(visible_height)).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme.muted))
            .track_style(Style::default().fg(theme.surface));
        f.render_stateful_widget(scrollbar, list_area, &mut state);
    }
}

fn render_detail_pane(f: &mut Frame, area: Rect, app: &App, theme: &Theme, active: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme.surface));
    let inner = separator.inner(area);
    f.render_widget(separator, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);
    render_pane_chip(f, parts[0], &app.files.selected_label(), active, theme);

    let mut logical = Vec::new();
    if let Some(path) = &app.files.selected_file {
        logical.push(format!("path   {}", path.display()));
    }
    if let Some(meta) = &app.files.selected_meta {
        logical.push(format!("size   {}", format_size(meta.size_bytes)));
        if let Some(modified) = &meta.modified {
            logical.push(format!("mtime  {}", modified));
        }
    }
    if app.files.selected_file.is_some() {
        logical.push(String::new());
    }

    if let Some(error) = &app.files.detail_error {
        logical.push(error.clone());
    } else {
        match &app.files.selected_kind {
            Some(DetailKind::Text { content }) => {
                logical.push("preview".to_string());
                logical.push(String::new());
                logical.extend(content.lines().map(str::to_string));
            }
            Some(DetailKind::Binary { reason }) => logical.push(reason.clone()),
            Some(DetailKind::TooLarge { size_bytes }) => {
                logical.push(format!(
                    "preview disabled for {} file",
                    format_size(*size_bytes)
                ));
            }
            None => logical.push("Select a file and press Enter.".to_string()),
        }
    }

    let wrapped = wrap_lines(&logical, parts[1].width as usize);
    let visible_height = parts[1].height as usize;
    let scroll = app
        .files
        .detail_scroll
        .min(wrapped.len().saturating_sub(visible_height));
    let lines: Vec<Line> = wrapped[scroll..wrapped.len().min(scroll + visible_height)]
        .iter()
        .map(|line| {
            let style = if line == "preview" {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else if app.files.detail_error.is_some() {
                Style::default().fg(theme.error)
            } else if line.starts_with("path   ")
                || line.starts_with("size   ")
                || line.starts_with("mtime  ")
            {
                Style::default().fg(theme.muted)
            } else {
                Style::default().fg(theme.fg)
            };
            Line::from(Span::styled(line.clone(), style))
        })
        .collect();

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), parts[1]);
}

fn render_pane_chip(f: &mut Frame, area: Rect, label: &str, active: bool, theme: &Theme) {
    let style = if active {
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
        Paragraph::new(Line::from(Span::styled(format!(" {label} ",), style))),
        area,
    );
}

fn build_tree_item(
    entry: &FlatEntry,
    selected: bool,
    active: bool,
    theme: &Theme,
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
    let prefix = if selected && active { "▸ " } else { "  " };
    let indent = "  ".repeat(entry.depth);
    let marker = if entry.is_dir {
        if entry.expanded {
            "▾ "
        } else {
            "▸ "
        }
    } else {
        "  "
    };
    let line = format!("{prefix}{indent}{marker}{}", entry.name);
    ListItem::new(Line::from(Span::styled(line, style)))
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
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    }
}

fn compact_text(text: &str, width: usize) -> String {
    if width <= 3 || text.len() <= width {
        return text.to_string();
    }
    format!("...{}", &text[text.len() - (width - 3)..])
}
