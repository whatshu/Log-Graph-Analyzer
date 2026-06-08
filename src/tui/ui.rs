use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
use ratatui::Frame;

use log_analyzer_core::repo::LogRepo;

use super::app::{App, InputMode, ViewKind};
use super::file_browser;

const COLOR_FG: Color = Color::White;
const COLOR_ACCENT: Color = Color::Cyan;
const COLOR_HIGHLIGHT: Color = Color::Yellow;
const COLOR_STATUS_BG: Color = Color::Rgb(0, 80, 120);
const COLOR_ERROR: Color = Color::Red;
const COLOR_LINE_NUMBER: Color = Color::DarkGray;
const COLOR_DIM: Color = Color::Gray;

pub fn render(f: &mut Frame, app: &mut App) {
    // Track terminal dimensions for horizontal scroll calculations
    let area = f.area();
    app.terminal_width = area.width;

    // File browser takes over the full screen
    if app.input_mode == InputMode::FileBrowser {
        render_file_browser_fullscreen(f, f.area(), app);
        return;
    }

    let has_input = matches!(app.input_mode, InputMode::Command | InputMode::Search | InputMode::Input);
    let constraints: Vec<Constraint> = if has_input {
        vec![
            Constraint::Length(1), // tabs
            Constraint::Min(1),    // main content
            Constraint::Length(1), // status bar
            Constraint::Length(1), // action bar (normal) or input bar
        ]
    } else {
        vec![
            Constraint::Length(1), // tabs
            Constraint::Min(1),    // main content
            Constraint::Length(1), // status bar
            Constraint::Length(1), // action bar (always visible in normal mode)
        ]
    };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(f.area());

    render_tabs(f, main_chunks[0], app);
    render_content(f, main_chunks[1], app);
    render_status(f, main_chunks[2], app);

    // Bottom bar: input or action hints
    if has_input {
        render_input(f, main_chunks[3], app);
    } else {
        render_action_bar(f, main_chunks[3], app);
    }

    if app.show_help {
        render_help_overlay(f, f.area(), app);
    }
}

/// Context-sensitive action bar showing available non-vim operations.
fn render_action_bar(f: &mut Frame, area: Rect, app: &App) {
    let hints = match app.active_view {
        ViewKind::LogView => vec![
            ("← → ^ $:HScroll", COLOR_DIM),
            ("/:Search", COLOR_ACCENT),
            ("f/F:Filter", COLOR_ACCENT),
            (":Cmd", COLOR_ACCENT),
            ("u:Undo", COLOR_ACCENT),
            ("i:Import", COLOR_HIGHLIGHT),
            ("e:Export", COLOR_HIGHLIGHT),
            ("h:History", COLOR_HIGHLIGHT),
            ("r:Repos", COLOR_DIM),
            ("s:Stats", COLOR_DIM),
            ("?:Help", COLOR_DIM),
            ("q:Quit", COLOR_ERROR),
        ],
        ViewKind::RepoList => vec![
            ("Enter:Open", COLOR_HIGHLIGHT),
            ("i:Import", COLOR_HIGHLIGHT),
            ("c:Clone", COLOR_ACCENT),
            ("d:Delete", COLOR_ERROR),
            ("l:Log", COLOR_DIM),
        ],
        ViewKind::History => vec![
            ("↑↓:Select", COLOR_ACCENT),
            ("Enter:Checkout", COLOR_HIGHLIGHT),
            ("e:Export", COLOR_HIGHLIGHT),
            ("u:Undo", COLOR_ACCENT),
            ("l:Log", COLOR_DIM),
            ("?:Help", COLOR_DIM),
        ],
        ViewKind::Analytics => vec![
            ("l:Log", COLOR_DIM),
            ("h:History", COLOR_DIM),
            ("?:Help", COLOR_DIM),
        ],
        _ => vec![
            ("?:Help", COLOR_DIM),
            ("q:Quit", COLOR_ERROR),
        ],
    };

    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(text, color)| {
            vec![
                Span::styled(*text, Style::default().fg(*color).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
            ]
        })
        .collect();

    let p = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::Rgb(20, 20, 20)));
    f.render_widget(p, area);
}

fn render_file_browser_fullscreen(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(1),   // browser content
            Constraint::Length(1), // hints
        ])
        .split(area);

    // Header
    let header = Paragraph::new(Span::styled(
        " File Browser — select a log file to import ",
        Style::default().fg(COLOR_FG).bg(COLOR_ACCENT).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(header, chunks[0]);

    // Browser
    file_browser::render_file_browser(f, chunks[1], &app.file_browser);

    // Hints
    file_browser::render_file_browser_hints(f, chunks[2]);
}

fn render_tabs(f: &mut Frame, area: Rect, app: &App) {
    let titles = vec!["Log", "History", "Repos", "Stats"];
    let selected = match app.active_view {
        ViewKind::LogView | ViewKind::FileBrowser => 0,
        ViewKind::History => 1,
        ViewKind::RepoList => 2,
        ViewKind::Analytics => 3,
        ViewKind::Help => 0,
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
        ViewKind::LogView | ViewKind::FileBrowser => render_log_view(f, area, app),
        ViewKind::RepoList => render_repo_list(f, area, app),
        ViewKind::Analytics => render_analytics(f, area, app),
        ViewKind::History => render_history(f, area, app),
        ViewKind::Help => render_log_view(f, area, app),
    }
}

fn render_log_view(f: &mut Frame, area: Rect, app: &App) {
    let repo = app.repo.borrow();
    if repo.is_none() {
        let msg = "No repo open. Press 'i' to import a log file, 'r' to browse repos.";
        let p = Paragraph::new(msg)
            .block(Block::default().borders(Borders::ALL).title("Log View"))
            .style(Style::default().fg(COLOR_DIM));
        f.render_widget(p, area);
        return;
    }

    let line_num_width = if app.total_lines > 0 {
        (app.total_lines as f64).log10() as usize + 1
    } else {
        1
    };
    let line_num_width = line_num_width.max(4);
    let content_width = area.width.saturating_sub(line_num_width as u16 + 3);

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
            let content_slice = if app.horizontal_scroll > 0 {
                let chars: Vec<char> = content.chars().collect();
                let start = app.horizontal_scroll.min(chars.len());
                chars[start..].iter().collect::<String>()
            } else {
                content.clone()
            };
            let truncated = truncate_str(&content_slice, content_width as usize);

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
        .map(|name| {
            let marker = if *name == active { " * " } else { "   " };
            let style = if *name == app.repo_name {
                Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD)
            } else if *name == active {
                Style::default().fg(COLOR_HIGHLIGHT)
            } else {
                Style::default().fg(COLOR_FG)
            };
            Line::from(Span::styled(format!("{}{}", marker, name), style))
        })
        .collect();

    let title = format!(" Repositories ({}) — Enter to open ", repos.len());
    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}

fn render_history(f: &mut Frame, area: Rect, app: &App) {
    if app.history_nodes.is_empty() {
        let p = Paragraph::new("No operation history. Apply some operations first.")
            .block(Block::default().borders(Borders::ALL).title("Operation History"))
            .style(Style::default().fg(COLOR_DIM));
        f.render_widget(p, area);
        return;
    }

    let visible = area.height.saturating_sub(2) as usize;
    let start = app.history_cursor.saturating_sub(visible / 2);
    let end = (start + visible).min(app.history_nodes.len());
    let start = if end - start < visible && start > 0 {
        end.saturating_sub(visible)
    } else {
        start
    };

    let lines: Vec<Line> = app.history_nodes[start..end]
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let idx = start + i;
            let is_cursor = idx == app.history_cursor;
            let is_last = idx == app.history_nodes.len() - 1;

            let marker = if is_cursor {
                if is_last { " ● " } else { " ◉ " }
            } else if is_last {
                " ○ "
            } else {
                " ◦ "
            };

            let _connector = if idx < app.history_nodes.len() - 1 {
                "│"
            } else {
                " "
            };

            let style = if is_cursor {
                Style::default().fg(Color::Black).bg(COLOR_ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_FG)
            };

            let desc = if node.line_count > 0 {
                format!(
                    "{}{:>2}  {:<50} {:>10} lines  {}",
                    marker,
                    node.id,
                    truncate_str(&node.description, 50),
                    format_count(node.line_count),
                    node.applied_at,
                )
            } else {
                format!(
                    "{}{:>2}  {:<50} {:>10}  {}",
                    marker,
                    node.id,
                    truncate_str(&node.description, 50),
                    "",
                    node.applied_at,
                )
            };

            Line::from(vec![
                Span::styled(desc, style),
            ])
        })
        .collect();

    let title = format!(
        " Operation History — {} operations | ↑↓ select  Enter checkout  e export ",
        app.history_nodes.len()
    );
    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .style(Style::default().fg(COLOR_FG));
    f.render_widget(p, area);
}

fn render_analytics(f: &mut Frame, area: Rect, app: &App) {
    let repo = app.repo.borrow();
    if repo.is_none() {
        let p = Paragraph::new("No repo open. Open a repo first to view analytics.")
            .block(Block::default().borders(Borders::ALL).title("Stats"))
            .style(Style::default().fg(COLOR_DIM));
        f.render_widget(p, area);
        return;
    }

    let stats_text = match repo.as_ref().unwrap().processor().stats() {
        Ok(stats) => {
            format!(
                "Total Lines:  {}\nTotal Bytes:  {}\nAvg Length:   {:.1}\nMax Length:   {}\nMin Length:   {}\nChunks:       {}\n\nOperations:   {}",
                stats.total_lines,
                format_bytes(stats.total_bytes),
                stats.avg_line_len,
                stats.max_line_len,
                stats.min_line_len,
                stats.chunk_count,
                repo.as_ref().map(|r: &LogRepo| r.history().len()).unwrap_or(0),
            )
        }
        Err(e) => format!("Error: {}", e),
    };

    let p = Paragraph::new(stats_text)
        .block(Block::default().borders(Borders::ALL).title(" Stats "))
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
    let max_w = area.width.saturating_sub(3) as usize;
    let buffer = &app.input_buffer;
    let visible_buf = if buffer.len() > max_w {
        let start = buffer.len().saturating_sub(max_w.saturating_sub(2));
        &buffer[start..]
    } else {
        buffer.as_str()
    };

    // In search mode, render spaces as middle dots so users can spot accidental extra spaces
    let display_buf: String = if app.input_mode == InputMode::Search {
        visible_buf.chars().map(|c| if c == ' ' { '·' } else { c }).collect()
    } else {
        visible_buf.to_string()
    };

    let text = format!("{} {}", app.input_prompt, display_buf);
    let p = Paragraph::new(text).style(Style::default().fg(COLOR_FG).bg(Color::Rgb(30, 30, 30)));
    f.render_widget(p, area);
}

fn render_help_overlay(f: &mut Frame, area: Rect, _app: &App) {
    let help_text = vec![
        Line::from(Span::styled(" KEYBINDINGS ", Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(vec![Span::styled("  Navigation  ", Style::default().fg(COLOR_ACCENT))]),
        Line::from(vec![Span::styled("  j/k ↑/↓     ", Style::default().fg(COLOR_ACCENT)), Span::raw("Scroll down/up")]),
        Line::from(vec![Span::styled("  Ctrl+d/u    ", Style::default().fg(COLOR_ACCENT)), Span::raw("Page down/up")]),
        Line::from(vec![Span::styled("  gg / G      ", Style::default().fg(COLOR_ACCENT)), Span::raw("Go to first/last line")]),
        Line::from(""),
        Line::from(vec![Span::styled("  Search & Filter ", Style::default().fg(COLOR_ACCENT))]),
        Line::from(vec![Span::styled("  /            ", Style::default().fg(COLOR_ACCENT)), Span::raw("Search (regex)")]),
        Line::from(vec![Span::styled("  n / N        ", Style::default().fg(COLOR_ACCENT)), Span::raw("Next/prev match")]),
        Line::from(vec![Span::styled("  f / F        ", Style::default().fg(COLOR_ACCENT)), Span::raw("Filter keep/remove")]),
        Line::from(vec![Span::styled("  R            ", Style::default().fg(COLOR_ACCENT)), Span::raw("Replace (uses search pattern)")]),
        Line::from(vec![Span::styled("  u            ", Style::default().fg(COLOR_ACCENT)), Span::raw("Undo last operation")]),
        Line::from(vec![Span::styled("  :            ", Style::default().fg(COLOR_ACCENT)), Span::raw("Command mode")]),
        Line::from(""),
        Line::from(vec![Span::styled("  Views       ", Style::default().fg(COLOR_ACCENT))]),
        Line::from(vec![Span::styled("  l            ", Style::default().fg(COLOR_ACCENT)), Span::raw("Log view")]),
        Line::from(vec![Span::styled("  h            ", Style::default().fg(COLOR_ACCENT)), Span::raw("History tree")]),
        Line::from(vec![Span::styled("  r            ", Style::default().fg(COLOR_ACCENT)), Span::raw("Repo list")]),
        Line::from(vec![Span::styled("  s            ", Style::default().fg(COLOR_ACCENT)), Span::raw("Stats")]),
        Line::from(vec![Span::styled("  i            ", Style::default().fg(COLOR_ACCENT)), Span::raw("Import file (browser)")]),
        Line::from(vec![Span::styled("  e            ", Style::default().fg(COLOR_ACCENT)), Span::raw("Export current state")]),
        Line::from(""),
        Line::from(vec![Span::styled("  Other       ", Style::default().fg(COLOR_ACCENT))]),
        Line::from(vec![Span::styled("  ?            ", Style::default().fg(COLOR_ACCENT)), Span::raw("This help")]),
        Line::from(vec![Span::styled("  q / Ctrl+C   ", Style::default().fg(COLOR_ACCENT)), Span::raw("Quit")]),
        Line::from(""),
        Line::from(vec![Span::styled("  Commands (:)", Style::default().fg(COLOR_ACCENT))]),
        Line::from(vec![Span::styled("  :f <pat>     ", Style::default().fg(COLOR_ACCENT)), Span::raw("Filter keep")]),
        Line::from(vec![Span::styled("  :fr <pat>    ", Style::default().fg(COLOR_ACCENT)), Span::raw("Filter remove")]),
        Line::from(vec![Span::styled("  :r /pat/repl/", Style::default().fg(COLOR_ACCENT)), Span::raw("Replace")]),
        Line::from(vec![Span::styled("  :w <path>    ", Style::default().fg(COLOR_ACCENT)), Span::raw("Export to file")]),
        Line::from(vec![Span::styled("  :d <idx>...  ", Style::default().fg(COLOR_ACCENT)), Span::raw("Delete lines")]),
        Line::from(vec![Span::styled("  :repo <name> ", Style::default().fg(COLOR_ACCENT)), Span::raw("Switch repo")]),
    ];

    let overlay_w = 60.min(area.width);
    let overlay_h = (help_text.len() as u16 + 2).min(area.height);
    let overlay_area = Rect {
        x: (area.width.saturating_sub(overlay_w)) / 2,
        y: (area.height.saturating_sub(overlay_h)) / 2,
        width: overlay_w,
        height: overlay_h,
    };

    // Clear the background first so underlying content doesn't bleed through
    f.render_widget(Clear, overlay_area);

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

fn format_count(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
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
