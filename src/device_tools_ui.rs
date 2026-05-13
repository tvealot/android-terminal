use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::device_tools::DeviceToolsDialog;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(dialog) = &app.device_tools else {
        return;
    };
    let width = area.width.min(100);
    let height = area.height.min(30);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let block = Block::default()
        .title(Span::styled(
            " device tools ",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(5)])
        .split(inner);
    render_toolbar(f, rows[0], app, dialog, theme);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(rows[1]);
    render_packages(f, cols[0], dialog, theme);
    render_result(f, cols[1], dialog, theme);
}

fn render_toolbar(f: &mut Frame, area: Rect, app: &App, dialog: &DeviceToolsDialog, theme: &Theme) {
    let device = app
        .current_device()
        .unwrap_or_else(|| "(default adb device)".to_string());
    let target = app.target_package.as_deref().unwrap_or("(unset)");
    let roots = dialog
        .scan_roots
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let lines = vec![
        Line::from(vec![
            Span::styled("device ", Style::default().fg(theme.muted)),
            Span::styled(device, Style::default().fg(theme.accent)),
            Span::styled("  target ", Style::default().fg(theme.muted)),
            Span::styled(target.to_string(), Style::default().fg(theme.warn)),
        ]),
        Line::from(vec![
            key("s", theme),
            Span::raw(" scrcpy  "),
            key("r", theme),
            Span::raw(" record 30s  "),
            key("w", theme),
            Span::raw(" wifi adb  "),
            key("i", theme),
            Span::raw(" install apk"),
        ]),
        Line::from(vec![
            key("l", theme),
            Span::raw(" launch  "),
            key("f", theme),
            Span::raw(" force-stop  "),
            key("c", theme),
            Span::raw(" clear data  "),
            key("p/Enter", theme),
            Span::raw(" set target  "),
            key("u", theme),
            Span::raw(" uninstall  "),
            key("!", theme),
            Span::raw(" confirm  "),
            key("R", theme),
            Span::raw(" rescan  "),
            key("Esc", theme),
            Span::raw(" close"),
        ]),
        Line::from(vec![
            Span::styled("scan ", Style::default().fg(theme.muted)),
            Span::styled(
                truncate(&roots, area.width.saturating_sub(5) as usize),
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(if dialog.loading {
            Span::styled(
                "scanning Android projects...",
                Style::default().fg(theme.warn),
            )
        } else if dialog.running {
            Span::styled("running device command...", Style::default().fg(theme.warn))
        } else {
            Span::styled(
                "j/k or arrows navigate packages",
                Style::default().fg(theme.muted),
            )
        }),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_packages(f: &mut Frame, area: Rect, dialog: &DeviceToolsDialog, theme: &Theme) {
    let block = Block::default()
        .title(Span::styled(
            format!(" packages {} ", dialog.packages.len()),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme.surface));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = Vec::new();
    if dialog.packages.is_empty() {
        let text = if dialog.loading {
            "Scanning Gradle projects and manifests..."
        } else {
            "No packages found. Pick or save a workspace, or keep Android projects under ~/Documents."
        };
        lines.push(Line::from(Span::styled(
            text,
            Style::default().fg(theme.muted),
        )));
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        return;
    }

    let visible = inner.height as usize;
    let start = if dialog.selected >= visible {
        dialog.selected + 1 - visible
    } else {
        0
    };
    for (i, pkg) in dialog.packages.iter().enumerate().skip(start).take(visible) {
        let selected = i == dialog.selected;
        let pending = dialog
            .pending_confirm
            .as_ref()
            .is_some_and(|p| p.package == pkg.package);
        let style = if selected {
            Style::default()
                .fg(theme.bg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if pending {
            Style::default().fg(theme.warn).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };
        let marker = if selected { ">" } else { " " };
        let pending_label = if pending { " !" } else { "" };
        let title = format!("{marker} {}{}", pkg.package, pending_label);
        lines.push(Line::from(vec![
            Span::styled(" ", style),
            Span::styled(truncate(&title, inner.width as usize), style),
        ]));
        if inner.height > 8 {
            let meta = format!("   {}  {}", pkg.project_name, pkg.source);
            lines.push(Line::from(Span::styled(
                truncate(&meta, inner.width as usize),
                Style::default().fg(theme.muted),
            )));
        }
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_result(f: &mut Frame, area: Rect, dialog: &DeviceToolsDialog, theme: &Theme) {
    let mut lines = Vec::new();
    if let Some(pkg) = dialog.selected_package() {
        lines.push(Line::from(vec![
            Span::styled("selected ", Style::default().fg(theme.muted)),
            Span::styled(
                pkg.package.clone(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            truncate(&pkg.path_label(), area.width as usize),
            Style::default().fg(theme.muted),
        )));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        "Package actions:",
        Style::default().fg(theme.muted),
    )));
    lines.push(Line::from("  install latest Gradle APK from this project"));
    lines.push(Line::from("  launch, force-stop, clear app data"));
    lines.push(Line::from(
        "  set shared target package from scanned project",
    ));
    lines.push(Line::from(""));

    if let Some(result) = &dialog.last {
        let color = if result.success {
            theme.success
        } else {
            theme.error
        };
        let package = result
            .package
            .as_ref()
            .map(|p| format!("  {p}"))
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(
                result.action.label(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(package, Style::default().fg(theme.muted)),
        ]));
        lines.push(Line::from(Span::styled(
            result.summary.clone(),
            Style::default().fg(color),
        )));
        lines.push(Line::from(""));
        let remaining = area.height.saturating_sub(lines.len() as u16) as usize;
        for line in result.output.lines().take(remaining) {
            lines.push(Line::from(Span::styled(
                truncate(line, area.width as usize),
                Style::default().fg(theme.fg),
            )));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Use `c` or `u`, then `!`, for destructive actions.",
            Style::default().fg(theme.warn),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn key(label: &str, theme: &Theme) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
    )
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{head}...")
    }
}
