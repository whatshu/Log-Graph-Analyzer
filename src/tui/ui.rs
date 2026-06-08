use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::Frame;

use log_analyzer_core::repo::LogRepo;

use super::app::{App, InputMode, ViewKind};

const _COLOR_BG: Color = Color::Black;
const COLOR_FG: Color = Color::White;
const COLOR_ACCENT: Color = Color::Cyan;
const COLOR_HIGHLIGHT: Color = Color::Yellow;
const COLOR_STATUS_BG: Color = Color::Rgb(0, 80, 120);
const COLOR_ERROR: Color = Color::Red;
const COLOR_LINE_NUMBER: Color = Color::DarkGray;
const COLOR_DIM: Color = Color::Gray;

pub fn render(f: &mut Frame, app: &App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tabs
            Constraint::Min(1),    // main content
            Constraint::Length(1), // status bar
            Constraint::Length(1), // input bar (only when needed)
        ])
        .split(f.area());

    render_tabs(f, main_chunks[0], app);
    render_content(f, main_chunks[1], app);
    render_status(f, main_chunks[2], app);

    // Input bar (conditional)
    match app.input_mode {
        InputMode::Command | InputMode::Search | InputMode::Input => {
            render_input(f, main_chunks[3], app);
        }
        _ => {}
    }

    // Help overlay
    if app.show_help {
        render_help_overlay(f, f.area(), app);
    }
}

fn render_tabs(f: &mut Frame, area: Rect, app: &App) {
    let titles = vec!["Log", "Repos", "Analytics"];
    let selected = match app.active_view {
        ViewKind::LogView => 0,
        ViewKind::RepoList => 1,
        ViewKind::Analytics => 2,
        ViewKind::Help => 0, // Help is an overlay
    };

    let tabs = Tabs::new(titles)
        .select(selected)
        .style(Style::default().fg(COLOR_DIM))
        .highlight_style(
            Style::default()
                .fg(COLOR_ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" ");

    f.render_widget(tabs, area);
}

fn render_content(f: &mut Frame, area: Rect, app: &App) {
    match app.active_view {
        ViewKind::LogView => render_log_view(f, area, app),
        ViewKind::RepoList => render_repo_list(f, area, app),
        ViewKind::Analytics => render_analytics(f, area, app),
        ViewKind::Help => render_log_view(f, area, app), // Help is overlay
    }
}

fn render_log_view(f: &mut Frame, area: Rect, app: &App) {
    let repo = app.repo.borrow();
    if repo.is_none() {
        let msg = "No repo open. Press 'i' to import a log file, or 'r' to browse repos.";
        let p = Paragraph::new(msg)
            .block(Block::default().borders(Borders::ALL).title("Log View"))
            .style(Style::default().fg(COLOR_DIM));
        f.render_widget(p, area);
        return;
    }

    // Calculate line number width
    let line_num_width = if app.total_lines > 0 {
        (app.total_lines as f64).log10() as usize + 1
    } else {
        1
    };
    let line_num_width = line_num_width.max(4);
    let content_width = area.width.saturating_sub(line_num_width as u16 + 3); // space + border

    let title = format!(
        " {} — {} lines ({} ops) ",
        app.repo_name,
        app.total_lines,
        repo.as_ref().map(|r: &LogRepo| r.history().len()).unwrap_or(0)
    );

    let lines: Vec<Line> = app
        .viewport_lines
        .iter()
        .enumerate()
        .map(|(i, content)| {
            let global_line = app.scroll_offset + i;
            let is_match = app.search_results.contains(&global_line);
            let is_cursor = global_line == app.cursor_line;

            let num_style = if is_cursor {
                Style::default()
                    .fg(Color::Black)
                    .bg(COLOR_ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_LINE_NUMBER)
            };

            let content_style = if is_match {
                Style::default()
                    .fg(Color::Black)
                    .bg(COLOR_HIGHLIGHT)
            } else if is_cursor {
                Style::default().fg(COLOR_FG).bg(Color::Rgb(40, 40, 40))
            } else {
                Style::default().fg(COLOR_FG)
            };

            let num = format!("{:>width$}", global_line + 1, width = line_num_width);
            let truncated = truncate_str(content, content_width as usize);

            Line::from(vec![
                Span::styled(format!(" {} ", num), num_style),
                Span::styled(truncated, content_style),
            ])
        })
        .collect();

    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .style(Style::default().fg(COLOR_FG)),
    );

    f.render_widget(p, area);
}

fn render_repo_list(f: &mut Frame, area: Rect, app: &App) {
    let repos = app.workspace.list().unwrap_or_default();
    let active = app.workspace.active().unwrap_or_default();

    if repos.is_empty() {
        let p = Paragraph::new("No repositories found. Press 'i' to import a log file.")
            .block(Block::default().borders(Borders::ALL).title("Repositories"))
            .style(Style::default().fg(COLOR_DIM));
        f.render_widget(p, area);
        return;
    }

    let lines: Vec<Line> = repos
        .iter()
        .enumerate()
        .map(|(_i, name)| {
            let marker = if *name == active { " * " } else { "   " };
            let style = if *name == app.repo_name {
                Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD)
            } else if *name == active {
                Style::default().fg(COLOR_HIGHLIGHT)
            } else {
                Style::default().fg(COLOR_FG)
            };
            Line::from(Span::styled(
                format!("{}{}", marker, name),
                style,
            ))
        })
        .collect();

    let title = format!(" Repositories ({}) — Enter to open ", repos.len());
    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}

fn render_analytics(f: &mut Frame, area: Rect, app: &App) {
    let repo = app.repo.borrow();
    if repo.is_none() {
        let p = Paragraph::new("No repo open. Open a repo first to view analytics.")
            .block(Block::default().borders(Borders::ALL).title("Analytics"))
            .style(Style::default().fg(COLOR_DIM));
        f.render_widget(p, area);
        return;
    }

    // Collect stats
    let stats_text = match repo.as_ref().unwrap().processor().stats() {
        Ok(stats) => {
            format!(
                "Total Lines: {}\nTotal Bytes: {}\nAvg Line Length: {:.1}\nMax Line Length: {}\nMin Line Length: {}\nChunks: {}",
                stats.total_lines,
                format_bytes(stats.total_bytes),
                stats.avg_line_len,
                stats.max_line_len,
                stats.min_line_len,
                stats.chunk_count,
            )
        }
        Err(e) => format!("Error collecting stats: {}", e),
    };

    let p = Paragraph::new(stats_text)
        .block(Block::default().borders(Borders::ALL).title(" Analytics "))
        .style(Style::default().fg(COLOR_FG));
    f.render_widget(p, area);
}

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let left = format!(
        " {} | L:{}/{} | ops:{} ",
        app.repo_name,
        app.cursor_line + 1,
        app.total_lines,
        app.repo.borrow().as_ref().map(|r: &LogRepo| r.history().len()).unwrap_or(0),
    );

    let (message, msg_style) = if let Some(ref err) = app.error_message {
        (err.as_str(), Style::default().fg(COLOR_ERROR))
    } else if !app.status_message.is_empty() {
        (app.status_message.as_str(), Style::default().fg(COLOR_ACCENT))
    } else {
        ("", Style::default())
    };

    let line = Line::from(vec![
        Span::styled(left, Style::default().fg(COLOR_FG).bg(COLOR_STATUS_BG)),
        Span::styled(message, msg_style.bg(COLOR_STATUS_BG)),
    ]);

    let p = Paragraph::new(line).style(Style::default().bg(COLOR_STATUS_BG));
    f.render_widget(p, area);
}

fn render_input(f: &mut Frame, area: Rect, app: &App) {
    let prompt = &app.input_prompt;
    let buffer = &app.input_buffer;
    let _cursor_pos = buffer.len();

    let max_w = area.width.saturating_sub(3) as usize;
    let visible_buf = if buffer.len() > max_w {
        let start = buffer.len().saturating_sub(max_w.saturating_sub(2));
        &buffer[start..]
    } else {
        buffer.as_str()
    };

    let text = format!("{} {}", prompt, visible_buf);
    let p = Paragraph::new(text).style(Style::default().fg(COLOR_FG).bg(Color::Rgb(30, 30, 30)));
    f.render_widget(p, area);
}

fn render_help_overlay(f: &mut Frame, area: Rect, _app: &App) {
    let help_text = vec![
        Line::from(Span::styled(
            " Keybindings ",
            Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  j / k        ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Scroll down / up"),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+d / u   ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Page down / up"),
        ]),
        Line::from(vec![
            Span::styled("  gg / G       ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Go to first / last line"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  /            ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Search forward (regex)"),
        ]),
        Line::from(vec![
            Span::styled("  n / N        ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Next / previous match"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  :            ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Enter command mode"),
        ]),
        Line::from(vec![
            Span::styled("  u            ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Undo last operation"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  r / l / a    ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Views: Repos / Log / Analytics"),
        ]),
        Line::from(vec![
            Span::styled("  i            ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Import log file"),
        ]),
        Line::from(vec![
            Span::styled("  e            ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Export current state"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ?            ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Toggle this help"),
        ]),
        Line::from(vec![
            Span::styled("  q / Ctrl+C   ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Quit"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " Commands ",
            Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  :f <regex>   ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Filter — keep matching lines"),
        ]),
        Line::from(vec![
            Span::styled("  :fr <regex>  ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Filter — remove matching lines"),
        ]),
        Line::from(vec![
            Span::styled("  :r /pat/repl/", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Replace regex"),
        ]),
        Line::from(vec![
            Span::styled("  :w <path>    ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Export current state to file"),
        ]),
        Line::from(vec![
            Span::styled("  :repo <name> ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Switch active repo"),
        ]),
        Line::from(vec![
            Span::styled("  :q           ", Style::default().fg(COLOR_ACCENT)),
            Span::raw("Quit"),
        ]),
    ];

    // Calculate overlay size
    let overlay_w = 50.min(area.width);
    let overlay_h = (help_text.len() as u16 + 2).min(area.height);
    let overlay_area = Rect {
        x: (area.width.saturating_sub(overlay_w)) / 2,
        y: (area.height.saturating_sub(overlay_h)) / 2,
        width: overlay_w,
        height: overlay_h,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Rgb(20, 20, 30)));
    let p = Paragraph::new(help_text).block(block);
    f.render_widget(p, overlay_area);
}

fn format_bytes(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.1} {}", size, UNITS[unit])
}

fn truncate_str(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut width = 0;
    let mut result = String::with_capacity(max_width);
    for c in s.chars() {
        let char_width = if c.is_ascii() { 1 } else { 2 };
        if width + char_width > max_width {
            break;
        }
        result.push(c);
        width += char_width;
    }
    result
}
