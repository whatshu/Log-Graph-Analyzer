use std::collections::HashSet;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
use ratatui::Frame;

use lograph::repo::LogRepo;

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

    if app.show_collect_detail {
        render_collect_detail(f, f.area(), app);
    }

    if app.show_tag_manager {
        render_tag_manager(f, f.area(), app);
    }
}

/// Context-sensitive action bar showing available keyboard shortcuts.
/// When a popup is active (tag manager, help, etc.), hints for that popup are shown.
/// Otherwise, hints reflect the current ViewKind.
fn render_action_bar(f: &mut Frame, area: Rect, app: &App) {
    let hints: Vec<(&str, Color)> = if app.show_tag_manager {
        vec![
            ("j/k:Select", COLOR_ACCENT),
            ("Enter:Activate", COLOR_HIGHLIGHT),
            ("r:Rename", COLOR_ACCENT),
            ("d:Delete", COLOR_ERROR),
            ("q/Esc:Close", COLOR_DIM),
        ]
    } else if app.show_help {
        vec![
            ("q/?/Esc:Close help", COLOR_ACCENT),
        ]
    } else {
        match app.active_view {
            ViewKind::LogView => vec![
                ("j/k:Scroll", COLOR_ACCENT),
                ("/:Search", COLOR_ACCENT),
                ("n/N:Match", COLOR_ACCENT),
                ("f/F:Filter", COLOR_ACCENT),
                ("R:Replace", COLOR_HIGHLIGHT),
                ("u:Undo", COLOR_HIGHLIGHT),
                ("c:Collect", COLOR_ACCENT),
                ("a:Append", COLOR_HIGHLIGHT),
                ("e:Export", COLOR_HIGHLIGHT),
                ("t:Tags", COLOR_DIM),
                ("h:History", COLOR_HIGHLIGHT),
                ("?:Help", COLOR_DIM),
                ("q:Quit", COLOR_ERROR),
            ],
            ViewKind::History => vec![
                ("j/k:Select", COLOR_ACCENT),
                ("gg/G:Top/Bot", COLOR_DIM),
                ("Space:Mark", COLOR_ACCENT),
                ("m:Merge", COLOR_HIGHLIGHT),
                ("d:Diff", COLOR_HIGHLIGHT),
                ("y:Yank", COLOR_ACCENT),
                ("p:Paste", COLOR_ACCENT),
                ("Enter:View", COLOR_HIGHLIGHT),
                ("u:Undo", COLOR_HIGHLIGHT),
                ("D:Del", COLOR_ERROR),
                ("l:Log", COLOR_DIM),
                ("?:Help", COLOR_DIM),
            ],
            ViewKind::RepoList => vec![
                ("j/k:Select", COLOR_ACCENT),
                ("Enter:Open", COLOR_HIGHLIGHT),
                ("i:Import", COLOR_HIGHLIGHT),
                ("c:Clone", COLOR_ACCENT),
                ("d:Delete", COLOR_ERROR),
                ("l:Log", COLOR_DIM),
                ("?:Help", COLOR_DIM),
            ],
            ViewKind::Analytics => vec![
                ("l:Log", COLOR_HIGHLIGHT),
                ("h:History", COLOR_HIGHLIGHT),
                ("r:Repos", COLOR_DIM),
                ("?:Help", COLOR_DIM),
                ("q:Quit", COLOR_ERROR),
            ],
            _ => vec![
                ("?:Help", COLOR_DIM),
                ("q:Quit", COLOR_ERROR),
            ],
        }
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
    file_browser::render_file_browser(f, chunks[1], &app.file_browser, app.ascii_only);

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

    // Build set of tagged line numbers
    let tagged_lines: HashSet<usize> = {
        let mut set = HashSet::new();
        for tag in app.tag_store.get_tags(&app.repo_name) {
            for &(s, e) in &tag.ranges {
                for line in s..=e {
                    set.insert(line);
                }
            }
        }
        set
    };

    let line_num_width = if app.total_lines > 0 {
        (app.total_lines as f64).log10() as usize + 1
    } else {
        1
    };
    let line_num_width = line_num_width.max(4);
    // Marker column is 2 chars: glyph + space
    let marker_width: u16 = 2;
    let content_width = area.width.saturating_sub(marker_width + line_num_width as u16 + 3);

    let title = format!(
        " {} — {} lines ({} ops) ",
        app.repo_name,
        app.total_lines,
        repo.as_ref().map(|r: &LogRepo| r.history_tree().len().saturating_sub(1)).unwrap_or(0)
    );

    let tag_marker_glyph = if app.ascii_only { "> " } else { "▸ " };

    let lines: Vec<Line> = app
        .viewport_lines
        .iter()
        .enumerate()
        .map(|(i, content)| {
            let global_line = app.scroll_offset + i;
            let is_match = app.search_results.contains(&global_line);
            let is_cursor = global_line == app.cursor_line;
            let is_tagged = tagged_lines.contains(&global_line);

            let num_style = if is_cursor {
                Style::default()
                    .fg(Color::Black)
                    .bg(COLOR_ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_LINE_NUMBER)
            };

            let marker_style = if is_tagged {
                Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(30, 30, 30))
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
                Span::styled(
                    if is_tagged { tag_marker_glyph.to_string() } else { "  ".to_string() },
                    marker_style,
                ),
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

    let repo_ref = app.repo.borrow();
    let current_branch = repo_ref
        .as_ref()
        .map(|r| r.current_branch().to_string())
        .unwrap_or_else(|| String::from("main"));
    drop(repo_ref);

    let lines: Vec<Line> = app.history_nodes[start..end]
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let idx = start + i;
            let is_cursor = idx == app.history_cursor;

            let mut spans: Vec<Span> = Vec::new();

            // Tree connector
            if !node.connector.is_empty() {
                spans.push(Span::styled(
                    format!("{} ", node.connector),
                    Style::default().fg(COLOR_DIM),
                ));
            }

            // Cursor / mark / deleted marker
            let is_marked = app.history_marks.contains(&node.id);
            let marker = if node.deleted {
                if app.ascii_only {
                    Span::styled("x ", Style::default().fg(COLOR_ERROR))
                } else {
                    Span::styled("✗ ", Style::default().fg(COLOR_ERROR))
                }
            } else if is_cursor && is_marked {
                if app.ascii_only {
                    Span::styled("* ", Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("◉ ", Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD))
                }
            } else if is_marked {
                if app.ascii_only {
                    Span::styled("+ ", Style::default().fg(COLOR_HIGHLIGHT).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("✓ ", Style::default().fg(COLOR_HIGHLIGHT).add_modifier(Modifier::BOLD))
                }
            } else if is_cursor {
                if app.ascii_only {
                    Span::styled("* ", Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("● ", Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD))
                }
            } else {
                if app.ascii_only {
                    Span::styled("o ", Style::default().fg(COLOR_DIM))
                } else {
                    Span::styled("◦ ", Style::default().fg(COLOR_DIM))
                }
            };

            // Node ID (colored by cursor)
            let id_style = if is_cursor {
                Style::default().fg(Color::Black).bg(COLOR_ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_DIM)
            };
            let id_span = Span::styled(format!("{:>2} ", node.id), id_style);

            // Description
            let desc_style = if is_cursor {
                Style::default().fg(COLOR_FG).add_modifier(Modifier::BOLD)
            } else if node.is_head {
                Style::default().fg(COLOR_ACCENT)
            } else if node.is_viewed {
                Style::default().fg(COLOR_HIGHLIGHT)
            } else {
                Style::default().fg(COLOR_FG)
            };
            let desc_text = truncate_str(&node.description, 45);
            let desc_span = Span::styled(format!("{:<45} ", desc_text), desc_style);

            // Line count
            let count_str = if node.line_count > 0 {
                format!("{:>8} lines", format_count(node.line_count))
            } else {
                String::from("         ")
            };
            let count_span = Span::styled(format!("{} ", count_str), Style::default().fg(COLOR_DIM));

            // Timestamp
            let ts_span = Span::styled(
                format!("{}", node.applied_at),
                Style::default().fg(COLOR_DIM),
            );

            // Branch labels
            let mut label_spans: Vec<Span> = Vec::new();
            for (bi, branch) in node.branch_labels.iter().enumerate() {
                let is_active = *branch == current_branch;
                let branch_color = if is_active {
                    COLOR_ACCENT
                } else {
                    COLOR_HIGHLIGHT
                };
                label_spans.push(Span::styled(
                    if app.ascii_only {
                        format!(" <{}", branch)
                    } else {
                        format!(" ◄{}", branch)
                    },
                    Style::default().fg(branch_color).add_modifier(Modifier::BOLD),
                ));
                if bi < node.branch_labels.len() - 1 {
                    label_spans.push(Span::raw(" "));
                }
            }

            // HEAD marker
            if node.is_head {
                label_spans.push(Span::styled(
                    " HEAD",
                    Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD),
                ));
            }

            // Viewed marker
            if node.is_viewed && !node.is_head {
                label_spans.push(Span::styled(
                    " [view]",
                    Style::default().fg(COLOR_HIGHLIGHT),
                ));
            }

            // Collect result summary
            if let Some(ref collect) = node.collect_summary {
                label_spans.push(Span::styled(
                    format!("  → {}", collect),
                    Style::default().fg(COLOR_DIM),
                ));
            }

            spans.push(marker);
            spans.push(id_span);
            spans.push(desc_span);
            spans.push(count_span);
            spans.push(ts_span);
            spans.extend(label_spans);

            Line::from(spans)
        })
        .collect();

    let title = format!(
        " Operation History — {} | {} ops | ↑↓ navigate  Enter view  H HEAD  e export ",
        current_branch,
        app.history_nodes.len().saturating_sub(1), // exclude root
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
                repo.as_ref().map(|r: &LogRepo| r.history_tree().len().saturating_sub(1)).unwrap_or(0),
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
        app.repo.borrow().as_ref().map(|r: &LogRepo| r.history_tree().len().saturating_sub(1)).unwrap_or(0),
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

    // In search mode, render spaces as visible dots so users can spot
    // accidental extra spaces. Uses middle-dot (·) in UTF-8 mode or
    // plain dot (.) in ASCII mode.
    let display_buf: String = if app.input_mode == InputMode::Search {
        let space_glyph = if app.ascii_only { '.' } else { '·' };
        visible_buf.chars().map(|c| if c == ' ' { space_glyph } else { c }).collect()
    } else {
        visible_buf.to_string()
    };

    let text = format!("{} {}", app.input_prompt, display_buf);
    let p = Paragraph::new(text).style(Style::default().fg(COLOR_FG).bg(Color::Rgb(30, 30, 30)));
    f.render_widget(p, area);
}

fn render_help_overlay(f: &mut Frame, area: Rect, app: &App) {
    let help_text: Vec<Line> = if app.show_tag_manager {
        build_tag_manager_help()
    } else {
        match app.active_view {
            ViewKind::LogView => build_log_view_help(),
            ViewKind::History => build_history_help(),
            ViewKind::RepoList => build_repo_list_help(),
            ViewKind::Analytics => build_analytics_help(),
            _ => build_log_view_help(),
        }
    };

    let overlay_w = 62.min(area.width);
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

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {}  ", title),
        Style::default().fg(COLOR_ACCENT),
    ))
}

fn key_line(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(key.to_string(), Style::default().fg(COLOR_ACCENT)),
        Span::raw(desc.to_string()),
    ])
}

fn build_log_view_help() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(" LOG VIEW KEYBINDINGS ", Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(""),
        section("Navigation"),
        key_line("  j/k ↑/↓     ", "  Scroll down/up"),
        key_line("  Ctrl+d/u    ", "  Page down/up"),
        key_line("  gg / G      ", "  Go to first/last line"),
        key_line("  ← → ^ $     ", "  Horizontal scroll / line start/end"),
        Line::from(""),
        section("Search & Filter"),
        key_line("  /            ", "  Search (regex)"),
        key_line("  n / N        ", "  Next/previous match"),
        key_line("  f / F        ", "  Filter keep/remove (uses search pattern)"),
        key_line("  R            ", "  Replace (uses search pattern)"),
        key_line("  u            ", "  Undo last operation"),
        key_line("  :            ", "  Command mode"),
        Line::from(""),
        section("Collect"),
        key_line("  c            ", "  Enter collect command (:collect ...)"),
        key_line("  count <pat>  ", "  Count lines matching pattern"),
        key_line("  group <p> <i>", "  Group count by capture group"),
        key_line("  topn <p> <i> <n>", "  Top-N by capture group"),
        key_line("  unique <p> <i>", "  Distinct values of group"),
        key_line("  numstats <p> <i>", "  Numeric stats (min/max/avg)"),
        key_line("  linestats    ", "  Line-length statistics"),
        Line::from(""),
        section("Edit"),
        key_line("  a            ", "  Append file to repo"),
        key_line("  I            ", "  Insert line after cursor"),
        key_line("  M            ", "  Modify current line"),
        Line::from(""),
        section("Tags"),
        key_line("  t            ", "  Toggle tag manager popup"),
        key_line("  T            ", "  Clear active tag scope"),
        Line::from(""),
        section("Views"),
        key_line("  l            ", "  Log view"),
        key_line("  h            ", "  History tree"),
        key_line("  r            ", "  Repo list"),
        key_line("  s            ", "  Stats / analytics"),
        key_line("  i            ", "  Import file (browser)"),
        key_line("  e            ", "  Export current state"),
        key_line("  H            ", "  Return to HEAD from history view"),
        Line::from(""),
        section("Commands"),
        key_line("  :f <pat>     ", "  Filter keep"),
        key_line("  :fr <pat>    ", "  Filter remove"),
        key_line("  :r /pat/repl/", "  Replace"),
        key_line("  :w <path>    ", "  Export to file"),
        key_line("  :d <idx>...  ", "  Delete lines"),
        key_line("  :i <N> <txt> ", "  Insert line after N"),
        key_line("  :m <N> <txt> ", "  Modify line N"),
        key_line("  :append <p>  ", "  Append file"),
        key_line("  :repo <name> ", "  Switch repo"),
        key_line("  :branch <n>  ", "  Create branch"),
        key_line("  :checkout <b>", "  Switch branch"),
        key_line("  :branches    ", "  List branches"),
        key_line("  :filters     ", "  List saved filters"),
        Line::from(""),
        section("Other"),
        key_line("  ?            ", "  This help"),
        key_line("  q / Ctrl+C   ", "  Quit"),
    ]
}

fn build_history_help() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(" HISTORY VIEW KEYBINDINGS ", Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(""),
        section("Navigation"),
        key_line("  j/k ↑/↓     ", "  Move cursor up/down"),
        key_line("  gg / G      ", "  Go to first/last node"),
        key_line("  Enter       ", "  View (checkout) selected node"),
        Line::from(""),
        section("Selection & Merge"),
        key_line("  Space       ", "  Mark node for multi-select merge"),
        key_line("  m           ", "  Merge marked nodes (OR union)"),
        Line::from(""),
        section("Diff & Subtract"),
        key_line("  d           ", "  Set diff base; press again to subtract"),
        Line::from(""),
        section("Copy & Paste"),
        key_line("  y           ", "  Yank (copy) node operation"),
        key_line("  p           ", "  Paste yanked operation at cursor"),
        key_line("  R           ", "  Replay node operation at cursor"),
        Line::from(""),
        section("Undo & Delete"),
        key_line("  u           ", "  Undo last operation"),
        key_line("  D           ", "  Soft-delete node"),
        Line::from(""),
        section("Views"),
        key_line("  l           ", "  Return to log view"),
        key_line("  H           ", "  Return to HEAD"),
        Line::from(""),
        section("Other"),
        key_line("  ?           ", "  This help"),
        key_line("  q / Ctrl+C  ", "  Quit"),
    ]
}

fn build_repo_list_help() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(" REPO LIST KEYBINDINGS ", Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(""),
        section("Navigation"),
        key_line("  j/k ↑/↓     ", "  Move cursor up/down"),
        Line::from(""),
        section("Repo Operations"),
        key_line("  Enter       ", "  Open selected repo"),
        key_line("  i           ", "  Import new file into a repo"),
        key_line("  c           ", "  Clone existing repo"),
        key_line("  d           ", "  Delete selected repo"),
        Line::from(""),
        section("Views"),
        key_line("  l           ", "  Return to log view"),
        Line::from(""),
        section("Other"),
        key_line("  ?           ", "  This help"),
        key_line("  q / Ctrl+C  ", "  Quit"),
    ]
}

fn build_analytics_help() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(" ANALYTICS KEYBINDINGS ", Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(""),
        section("Navigation"),
        key_line("  j/k ↑/↓     ", "  Scroll stats panel"),
        Line::from(""),
        section("Views"),
        key_line("  l           ", "  Log view"),
        key_line("  h           ", "  History tree"),
        key_line("  r           ", "  Repo list"),
        Line::from(""),
        section("Other"),
        key_line("  ?           ", "  This help"),
        key_line("  q / Ctrl+C  ", "  Quit"),
    ]
}

fn build_tag_manager_help() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(" TAG MANAGER KEYBINDINGS ", Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(""),
        section("Navigation"),
        key_line("  j/k ↑/↓     ", "  Move cursor up/down"),
        Line::from(""),
        section("Tag Operations"),
        key_line("  Enter       ", "  Activate selected tag as scope"),
        key_line("  r           ", "  Rename selected tag"),
        key_line("  d           ", "  Delete selected tag"),
        Line::from(""),
        section("Other"),
        key_line("  q / Esc     ", "  Close tag manager"),
        key_line("  ?           ", "  This help"),
    ]
}

fn render_collect_detail(f: &mut Frame, area: Rect, app: &App) {
    if app.collect_detail.is_none() {
        return;
    }

    let detail = app.collect_detail.as_ref().unwrap();
    let detail_lines: Vec<&str> = detail.lines().collect();
    // Count display width of the widest line
    let max_line_width: u16 = detail_lines
        .iter()
        .map(|l| l.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum::<usize>())
        .max()
        .unwrap_or(40) as u16;

    let overlay_w = (max_line_width + 6).min(area.width.saturating_sub(4)).max(30);
    let overlay_h = (detail_lines.len() as u16 + 4).min(area.height.saturating_sub(4));

    let overlay_area = Rect {
        x: (area.width.saturating_sub(overlay_w)) / 2,
        y: (area.height.saturating_sub(overlay_h)) / 2,
        width: overlay_w,
        height: overlay_h,
    };

    f.render_widget(Clear, overlay_area);

    let mut lines: Vec<Line> = detail_lines
        .iter()
        .take(overlay_h.saturating_sub(2) as usize)
        .map(|s| Line::from(Span::styled(*s, Style::default().fg(COLOR_FG))))
        .collect();

    // Add close hint at bottom
    lines.push(Line::from(Span::styled(
        " Press c/q/Esc to close",
        Style::default().fg(COLOR_DIM).add_modifier(Modifier::ITALIC),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Collect Result ")
        .style(Style::default().bg(Color::Rgb(20, 20, 30)));
    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, overlay_area);
}

fn render_tag_manager(f: &mut Frame, area: Rect, app: &App) {
    let tags = app.tag_store.get_tags(&app.repo_name);
    if tags.is_empty() {
        // Show a small popup indicating no tags
        let overlay_w = 40u16.min(area.width.saturating_sub(4));
        let overlay_h = 6u16.min(area.height.saturating_sub(4));
        let overlay_area = Rect {
            x: (area.width.saturating_sub(overlay_w)) / 2,
            y: (area.height.saturating_sub(overlay_h)) / 2,
            width: overlay_w,
            height: overlay_h,
        };
        f.render_widget(Clear, overlay_area);
        let lines = vec![
            Line::from(Span::styled("No tags created yet.", Style::default().fg(COLOR_DIM))),
            Line::from(""),
            Line::from(Span::styled("Tags are ranges you create via commands.", Style::default().fg(COLOR_DIM))),
            Line::from(Span::styled("See help (?) for tag operations.", Style::default().fg(COLOR_DIM))),
            Line::from(Span::styled(" q/Esc to close", Style::default().fg(COLOR_DIM).add_modifier(Modifier::ITALIC))),
        ];
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Tag Manager ")
            .style(Style::default().bg(Color::Rgb(20, 20, 30)));
        let p = Paragraph::new(lines).block(block);
        f.render_widget(p, overlay_area);
        return;
    }

    // Sort tags by start line
    let mut sorted_tags: Vec<&lograph::tag::Tag> = tags.iter().collect();
    sorted_tags.sort_by_key(|t| t.ranges.first().map(|&(s, _)| s).unwrap_or(0));

    let tag_count = sorted_tags.len();
    // Calculate popup size
    let max_tag_width = sorted_tags.iter()
        .map(|t| t.name.len() + 12) // name + "lines X-Y" overhead
        .max()
        .unwrap_or(30)
        .max(30) as u16;
    let overlay_w = (max_tag_width + 4).min(area.width.saturating_sub(4)).max(40);
    let overlay_h = (tag_count as u16 + 5).min(area.height.saturating_sub(4)).max(8);

    let overlay_area = Rect {
        x: (area.width.saturating_sub(overlay_w)) / 2,
        y: (area.height.saturating_sub(overlay_h)) / 2,
        width: overlay_w,
        height: overlay_h,
    };

    f.render_widget(Clear, overlay_area);

    // Read repo lines for content preview
    let tag_previews: Vec<String> = sorted_tags.iter().map(|tag| {
        if let Some(&(s, _)) = tag.ranges.first() {
            let repo_ref = app.repo.borrow();
            if let Some(ref r) = *repo_ref {
                if r.history_tree().is_empty() {
                    r.read_original_lines(s, 1).ok()
                        .and_then(|v| v.into_iter().next())
                        .unwrap_or_else(|| String::from("..."))
                } else {
                    // Can't call mutable method here; use viewport lines if available
                    let viewport_start = app.scroll_offset;
                    let viewport_end = viewport_start + app.viewport_lines.len();
                    if s >= viewport_start && s < viewport_end {
                        app.viewport_lines.get(s - viewport_start).cloned().unwrap_or_else(|| String::from("..."))
                    } else {
                        String::from("(scroll to view)")
                    }
                }
            } else {
                String::from("...")
            }
        } else {
            String::from("...")
        }
    }).collect();

    let visible_height = (overlay_h.saturating_sub(3)) as usize;
    let scroll_start = app.tag_manager_scroll.min(
        tag_count.saturating_sub(visible_height)
    );
    let scroll_end = (scroll_start + visible_height).min(tag_count);

    let mut lines: Vec<Line> = Vec::new();
    for i in scroll_start..scroll_end {
        let tag = sorted_tags[i];
        let is_selected = i == app.tag_manager_cursor;

        let range_str = if let Some(&(s, e)) = tag.ranges.first() {
            if tag.ranges.len() > 1 {
                format!("lines {}-{} (+{} more)", s + 1, e + 1, tag.ranges.len() - 1)
            } else {
                format!("lines {}-{}", s + 1, e + 1)
            }
        } else {
            String::from("(empty)")
        };

        let cursor_mark = if is_selected {
            if app.ascii_only { "> " } else { "▸ " }
        } else {
            "  "
        };

        let name_style = if is_selected {
            Style::default().fg(Color::Black).bg(COLOR_ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(COLOR_ACCENT).add_modifier(Modifier::BOLD)
        };
        let range_style = if is_selected {
            Style::default().fg(Color::Black).bg(COLOR_ACCENT)
        } else {
            Style::default().fg(COLOR_DIM)
        };
        let preview_style = if is_selected {
            Style::default().fg(Color::Black).bg(COLOR_ACCENT)
        } else {
            Style::default().fg(COLOR_DIM)
        };

        let name_text = format!("{}{:<20}", cursor_mark, tag.name);
        let range_text = format!(" {}", range_str);

        lines.push(Line::from(vec![
            Span::styled(name_text, name_style),
            Span::styled(range_text, range_style),
        ]));

        // Content preview
        let preview = &tag_previews[i];
        let preview_chars: Vec<char> = preview.chars().collect();
        let h_scroll = app.tag_manager_h_scroll.min(preview_chars.len());
        let preview_visible: String = preview_chars[h_scroll..].iter().take(overlay_w as usize - 4).collect();
        lines.push(Line::from(Span::styled(
            format!("  └ {}", preview_visible),
            preview_style,
        )));
    }

    // Bottom hints
    lines.push(Line::from(Span::styled(
        " j/k:nav Enter:jump d:del r:rename y:copy ←/→:hscroll q:close",
        Style::default().fg(COLOR_DIM).add_modifier(Modifier::ITALIC),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Tag Manager ({}) ", tag_count))
        .style(Style::default().bg(Color::Rgb(20, 20, 30)));
    let p = Paragraph::new(lines).block(block);
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
