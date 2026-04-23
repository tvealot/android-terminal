use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme, focused: bool) {
    let border_color = if focused { theme.accent } else { theme.surface };
    let status = if app.shell.active {
        " shell [adb] (Ctrl+\\ defocus)"
    } else if app.shell.last_error.is_some() {
        " shell [stopped]"
    } else {
        " shell (press `s` to focus, starts on focus)"
    };
    let title = status.to_string();

    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if !app.shell.active {
        let hint = if let Some(e) = &app.shell.last_error {
            format!(" {}\n Press `s` to restart.", e)
        } else {
            " Waiting to start. Focus the panel (`s` or 9).".to_string()
        };
        let p = Paragraph::new(hint).style(Style::default().fg(theme.muted));
        f.render_widget(p, inner);
        return;
    }

    let Ok(parser) = app.shell.parser.lock() else {
        return;
    };
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let render_rows = rows.min(inner.height);
    let render_cols = cols.min(inner.width);

    let mut lines: Vec<Line> = Vec::with_capacity(render_rows as usize);
    for r in 0..render_rows {
        let mut spans: Vec<Span> = Vec::new();
        let mut run = String::new();
        let mut run_style = Style::default().fg(theme.fg);
        let mut c = 0u16;
        while c < render_cols {
            let Some(cell) = screen.cell(r, c) else {
                run.push(' ');
                c += 1;
                continue;
            };
            let style = cell_style(cell, theme);
            let contents = cell.contents();
            let text = if contents.is_empty() {
                " ".to_string()
            } else {
                contents
            };
            if style != run_style && !run.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut run), run_style));
                run_style = style;
            } else if run.is_empty() {
                run_style = style;
            }
            run.push_str(&text);
            c += if cell.is_wide() { 2 } else { 1 };
        }
        if !run.is_empty() {
            spans.push(Span::styled(run, run_style));
        }
        lines.push(Line::from(spans));
    }

    let p = Paragraph::new(lines);
    f.render_widget(p, inner);

    if focused {
        let (cur_row, cur_col) = screen.cursor_position();
        if cur_row < render_rows && cur_col < render_cols {
            f.set_cursor_position((inner.x + cur_col, inner.y + cur_row));
        }
    }
}

fn cell_style(cell: &vt100::Cell, theme: &Theme) -> Style {
    let mut style = Style::default();
    let fg = convert_color(cell.fgcolor()).unwrap_or(theme.fg);
    let bg = convert_color(cell.bgcolor());
    style = style.fg(fg);
    if let Some(b) = bg {
        style = style.bg(b);
    }
    let mut m = Modifier::empty();
    if cell.bold() {
        m |= Modifier::BOLD;
    }
    if cell.italic() {
        m |= Modifier::ITALIC;
    }
    if cell.underline() {
        m |= Modifier::UNDERLINED;
    }
    if cell.inverse() {
        m |= Modifier::REVERSED;
    }
    if !m.is_empty() {
        style = style.add_modifier(m);
    }
    style
}

fn convert_color(c: vt100::Color) -> Option<Color> {
    match c {
        vt100::Color::Default => None,
        vt100::Color::Idx(i) => Some(Color::Indexed(i)),
        vt100::Color::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}
