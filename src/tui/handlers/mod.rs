use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use log_analyzer_core::operator::Operation;
use std::path::Path;

use super::app::{App, InputMode, ViewKind};

pub fn normal_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            match app.active_view {
                ViewKind::History => {
                    if app.history_cursor + 1 < app.history_nodes.len() {
                        app.history_cursor += 1;
                    }
                }
                _ => app.scroll_down(1),
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            match app.active_view {
                ViewKind::History => {
                    if app.history_cursor > 0 {
                        app.history_cursor -= 1;
                    }
                }
                _ => app.scroll_up(1),
            }
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.page_down();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.page_up();
        }
        KeyCode::Char('g') => {
            if app.active_view == ViewKind::History {
                app.history_cursor = 0;
            } else {
                app.go_to_line(0);
            }
        }
        KeyCode::Char('G') => {
            if app.active_view == ViewKind::History {
                app.history_cursor = app.history_nodes.len().saturating_sub(1);
            } else if app.total_lines > 0 {
                app.go_to_line(app.total_lines - 1);
            }
        }
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Search;
            app.input_buffer.clear();
            app.input_prompt = String::from("/");
        }
        KeyCode::Char(':') => {
            app.input_mode = InputMode::Command;
            app.input_buffer.clear();
            app.input_prompt = String::from(":");
        }
        KeyCode::Char('n') => app.next_match(),
        KeyCode::Char('N') => app.prev_match(),
        KeyCode::Char('u') => {
            if app.active_view == ViewKind::History {
                // Undo from history view
                app.queue_undo();
            } else {
                app.queue_undo();
            }
        }
        // Horizontal scroll
        KeyCode::Left => app.scroll_left(8),
        KeyCode::Right => app.scroll_right(8),
        KeyCode::Char('^') => app.go_to_line_start(),
        KeyCode::Char('$') => app.go_to_line_end(),

        // View switching
        KeyCode::Char('h') => {
            app.horizontal_scroll = 0;
            app.build_history();
            app.active_view = ViewKind::History;
        }
        KeyCode::Char('l') => {
            app.horizontal_scroll = 0;
            app.active_view = ViewKind::LogView;
            app.load_viewport();
        }
        KeyCode::Char('r') => {
            app.active_view = ViewKind::RepoList;
        }
        KeyCode::Char('s') => {
            app.active_view = ViewKind::Analytics;
        }
        KeyCode::Char('?') => app.show_help = !app.show_help,

        // f: apply current search pattern as filter-keep
        KeyCode::Char('f') => {
            if app.search_query.is_empty() {
                app.status_message = String::from("No active search — use / first");
            } else {
                let pattern = app.search_query.clone();
                app.add_to_history(&pattern);
                app.queue_operation(Operation::Filter {
                    pattern: pattern.clone(),
                    keep: true,
                });
                app.status_message = format!("Filter keep: {}", pattern);
            }
        }

        // F: filter-remove — inverse of f, removes matching lines
        KeyCode::Char('F') => {
            if app.search_query.is_empty() {
                app.status_message = String::from("No active search — use / first");
            } else {
                let pattern = app.search_query.clone();
                app.add_to_history(&pattern);
                app.queue_operation(Operation::Filter {
                    pattern: pattern.clone(),
                    keep: false,
                });
                app.status_message = format!("Filter remove: {}", pattern);
            }
        }

        // R: replace — use current search pattern, prompt for replacement
        KeyCode::Char('R') => {
            if app.search_query.is_empty() {
                app.status_message = String::from("No active search — use / first");
            } else {
                app.input_mode = InputMode::Input;
                app.input_buffer.clear();
                app.input_prompt = format!(
                    "Replace /{}/ → ",
                    app.search_query
                );
            }
        }

        // File browser for import
        KeyCode::Char('i') => app.open_file_browser(),

        // Export
        KeyCode::Char('e') => {
            if app.active_view == ViewKind::History {
                let node_idx = app.history_cursor;
                app.pending_history_export = Some(node_idx);
                let default_name = format!("export_op_{}.log", node_idx);
                app.input_mode = InputMode::Input;
                app.input_buffer = default_name;
                app.input_prompt = String::from("Export path: ");
            } else {
                app.pending_history_export = None;
                app.input_mode = InputMode::Input;
                app.input_buffer.clear();
                app.input_prompt = String::from("Export to file: ");
            }
        }

        // History view specific
        KeyCode::Enter => {
            if app.active_view == ViewKind::History {
                if app.history_cursor < app.history_nodes.len() {
                    app.queue_checkout(app.history_cursor);
                }
            } else if app.active_view == ViewKind::RepoList {
                if let Ok(repos) = app.workspace.list() {
                    if !repos.is_empty() {
                        let idx = 0usize; // Simplified — would need cursor in repo list
                        if idx < repos.len() {
                            app.open_repo(Some(&repos[idx]));
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

pub fn command_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let cmd = app.input_buffer.clone();
            app.input_buffer.clear();
            app.input_mode = InputMode::Normal;
            execute_command(app, &cmd);
        }
        KeyCode::Char(c) => app.input_buffer.push(c),
        KeyCode::Backspace => { app.input_buffer.pop(); }
        _ => {}
    }
}

pub fn search_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let query = app.input_buffer.clone();
            app.input_buffer.clear();
            app.input_mode = InputMode::Normal;
            app.history_reset();
            if !query.is_empty() {
                app.add_to_history(&query);
                app.do_search(&query);
            }
        }
        KeyCode::Up => {
            // Navigate search history (up = older)
            if let Some(term) = app.history_navigate_up() {
                app.input_buffer = term.to_string();
            }
        }
        KeyCode::Down => {
            // Navigate search history (down = newer)
            if let Some(term) = app.history_navigate_down() {
                app.input_buffer = term.to_string();
            }
        }
        KeyCode::Char(c) => {
            app.history_reset();
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => {
            app.history_reset();
            app.input_buffer.pop();
        }
        _ => {}
    }
}

pub fn input_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let input = app.input_buffer.clone();
            let prompt = app.input_prompt.clone();
            app.input_buffer.clear();
            app.input_mode = InputMode::Normal;
            handle_input(app, &prompt, &input);
        }
        KeyCode::Char(c) => app.input_buffer.push(c),
        KeyCode::Backspace => { app.input_buffer.pop(); }
        _ => {}
    }
}

/// Handle file browser mode keys.
pub fn file_browser_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.status_message = String::from("File browser cancelled");
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.file_browser.move_down();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.file_browser.move_up();
        }
        KeyCode::Char('h') | KeyCode::Left => {
            // Go to parent directory
            if let Some(parent) = app.file_browser.current_dir.parent() {
                app.file_browser.current_dir = parent.to_path_buf();
                app.file_browser.selected_index = 0;
                app.file_browser.scroll_offset = 0;
                app.file_browser.refresh();
            }
        }
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
            if app.file_browser.enter_dir() {
                // File selected — import it
                app.import_from_file_browser();
            }
        }
        KeyCode::Char('.') => {
            app.file_browser.toggle_hidden();
        }
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Search;
            app.input_buffer.clear();
            app.input_prompt = String::from("Filter: ");
            // After search, apply filter
        }
        KeyCode::Char('g') => {
            app.file_browser.selected_index = 0;
            app.file_browser.scroll_offset = 0;
        }
        KeyCode::Char('G') => {
            let last = app.file_browser.entries.len().saturating_sub(1);
            app.file_browser.selected_index = last;
        }
        _ => {}
    }
}

fn execute_command(app: &mut App, cmd: &str) {
    let cmd = cmd.trim();
    if cmd.is_empty() { return; }

    if cmd == "q" {
        app.should_quit = true;
    } else if let Some(pattern) = cmd.strip_prefix("f ") {
        let resolved = resolve_pattern(&app.config, pattern);
        app.add_to_history(&resolved);
        app.queue_operation(Operation::Filter {
            pattern: resolved.clone(),
            keep: true,
        });
        app.status_message = format!("Filter keep: {}", resolved);
        app.search_query = resolved;
    } else if let Some(pattern) = cmd.strip_prefix("fr ") {
        let resolved = resolve_pattern(&app.config, pattern);
        app.add_to_history(&resolved);
        app.queue_operation(Operation::Filter {
            pattern: resolved.clone(),
            keep: false,
        });
        app.status_message = format!("Filter remove: {}", resolved);
        app.search_query = resolved;
    } else if let Some(args) = cmd.strip_prefix("r ") {
        if let Some(inner) = parse_delimited(args, '/') {
            let parts: Vec<&str> = inner.splitn(2, '/').collect();
            if parts.len() == 2 {
                let pattern = parts[0].to_string();
                app.add_to_history(&pattern);
                app.queue_operation(Operation::Replace {
                    pattern: pattern.clone(),
                    replacement: parts[1].to_string(),
                });
                app.status_message = format!("Replace /{}/ -> {}", pattern, parts[1]);
                app.search_query = pattern;
            } else {
                app.error_message = Some("Invalid replace syntax. Use :r /pat/repl/".to_string());
            }
        } else {
            app.error_message = Some("Invalid replace syntax. Use :r /pat/repl/".to_string());
        }
    } else if let Some(indices_str) = cmd.strip_prefix("d ") {
        let indices: Vec<usize> = indices_str
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if indices.is_empty() {
            app.error_message = Some("Usage: :d <line_number>...".to_string());
        } else {
            // Convert 1-based UI indices to 0-based
            let zero_based: Vec<usize> = indices.iter().map(|i| i.saturating_sub(1)).collect();
            app.queue_operation(Operation::DeleteLines {
                line_indices: zero_based,
            });
            app.status_message = format!("Delete {} lines", indices.len());
        }
    } else if let Some(path) = cmd.strip_prefix("w ") {
        let path = path.trim();
        let mut repo_mut = app.repo.borrow_mut();
        if let Some(ref mut r) = *repo_mut {
            match r.export(Path::new(path)) {
                Ok(()) => app.status_message = format!("Exported to {}", path),
                Err(e) => app.error_message = Some(format!("Export failed: {}", e)),
            }
        } else {
            app.error_message = Some("No repo open".to_string());
        }
    } else if let Some(name) = cmd.strip_prefix("repo ") {
        let name = name.trim();
        app.open_repo(Some(name));
    } else if cmd == "filters" {
        let names = app.config.filter_names();
        if names.is_empty() {
            app.status_message = String::from("No saved filters. Add them to ~/.log_analyzer/config.toml");
        } else {
            let list: Vec<String> = names
                .iter()
                .map(|n| {
                    let pat = app.config.get_filter(n).unwrap_or("");
                    format!("  @{} = \"{}\"", n, pat)
                })
                .collect();
            // Show first few in status; full list is too long
            app.status_message = format!("Saved filters: {}", list.join(", "));
        }
    } else {
        app.error_message = Some(format!("Unknown command: {}", cmd));
    }
}

/// Resolve a pattern that may be a @name reference to a saved filter.
fn resolve_pattern(config: &log_analyzer_core::config::Config, pattern: &str) -> String {
    let trimmed = pattern.trim();
    if let Some(name) = trimmed.strip_prefix('@') {
        config.get_filter(name).map(|s| s.to_string()).unwrap_or_else(|| {
            trimmed.to_string()
        })
    } else {
        trimmed.to_string()
    }
}

fn parse_delimited(s: &str, delim: char) -> Option<&str> {
    let s = s.strip_prefix(delim)?;
    let end = s.find(delim)?;
    Some(&s[..end])
}

fn handle_input(app: &mut App, prompt: &str, input: &str) {
    if prompt.starts_with("Replace /") {
        let pattern = app.search_query.clone();
        app.add_to_history(&pattern);
        app.queue_operation(Operation::Replace {
            pattern: pattern.clone(),
            replacement: input.to_string(),
        });
        app.status_message = format!("Replace /{}/ → {}", pattern, input);
    } else if prompt.contains("Import") {
        let path = Path::new(input);
        if !path.exists() {
            app.error_message = Some(format!("File not found: {}", input));
            return;
        }
        let name = path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| String::from("default"));
        match app.workspace.import_file(&name, path) {
            Ok(repo) => {
                app.total_lines = repo.original_line_count();
                app.line_count_is_original = true;
                app.repo_name = name.clone();
                *app.repo.borrow_mut() = Some(repo);
                app.scroll_offset = 0;
                app.cursor_line = 0;
                app.active_view = ViewKind::LogView;
                app.load_viewport();
                app.status_message = format!("Imported '{}' as '{}'", input, name);
            }
            Err(e) => app.error_message = Some(format!("Import failed: {}", e)),
        }
    } else if prompt.contains("Export path") {
        let node_idx = app.pending_history_export.take().unwrap_or(0);
        app.queue_export_from(node_idx, input.to_string());
    } else if prompt.contains("Export") {
        let mut repo_mut = app.repo.borrow_mut();
        if let Some(ref mut r) = *repo_mut {
            match r.export(Path::new(input)) {
                Ok(()) => app.status_message = format!("Exported to {}", input),
                Err(e) => app.error_message = Some(format!("Export failed: {}", e)),
            }
        } else {
            app.error_message = Some("No repo open".to_string());
        }
    }
}
