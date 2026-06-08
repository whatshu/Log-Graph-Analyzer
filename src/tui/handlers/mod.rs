use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lga_core::engine::Collector;
use lga_core::operator::Operation;
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
        // c: enter collect command mode
        KeyCode::Char('c') => {
            app.input_mode = InputMode::Command;
            app.input_buffer = String::from("collect ");
            app.input_prompt = String::from(":");
        }
        KeyCode::Char('h') => {
            app.horizontal_scroll = 0;
            app.build_history();
            app.active_view = ViewKind::History;
        }
        KeyCode::Char('l') => {
            app.horizontal_scroll = 0;
            // If viewing a historical node, return to HEAD first
            if app.viewed_node_id.is_some() {
                app.return_to_head();
            }
            app.active_view = ViewKind::LogView;
            app.load_viewport();
        }
        // H: return to HEAD from view mode (like Shift+H)
        KeyCode::Char('H') => {
            app.return_to_head();
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

        // a: append file to repo
        KeyCode::Char('a') => {
            if app.repo.borrow().is_none() {
                app.error_message = Some("No repo open".to_string());
            } else {
                app.input_mode = InputMode::Input;
                app.input_buffer.clear();
                app.input_prompt = String::from("Append file path: ");
            }
        }

        // I: insert line(s) after cursor
        KeyCode::Char('I') => {
            if app.repo.borrow().is_none() {
                app.error_message = Some("No repo open".to_string());
            } else {
                app.input_mode = InputMode::Input;
                app.input_buffer.clear();
                app.input_prompt = format!(
                    "Insert after line {}: ",
                    app.cursor_line + 1
                );
            }
        }

        // M: modify current line
        KeyCode::Char('M') => {
            if app.repo.borrow().is_none() {
                app.error_message = Some("No repo open".to_string());
            } else if app.viewport_lines.is_empty() {
                app.error_message = Some("No line to modify".to_string());
            } else {
                let line_idx = app.cursor_line;
                let current = app.repo.borrow().as_ref().and_then(|r| {
                    let lines = app.viewport_lines.clone();
                    let viewport_offset = line_idx.saturating_sub(app.scroll_offset);
                    lines.get(viewport_offset).cloned()
                }).unwrap_or_default();
                app.input_mode = InputMode::Input;
                app.input_buffer = current;
                app.input_prompt = format!("Modify line {}: ", line_idx + 1);
            }
        }

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
    } else if let Some(sub) = cmd.strip_prefix("collect ") {
        execute_collect(app, sub.trim());
    } else if let Some(sub) = cmd.strip_prefix("count ") {
        // Direct :count <pattern> — convenience alias
        execute_collect(app, &format!("count {}", sub.trim()));
    } else if cmd == "count" {
        execute_collect(app, "count");
    } else if let Some(sub) = cmd.strip_prefix("group ") {
        execute_collect(app, &format!("group {}", sub.trim()));
    } else if let Some(sub) = cmd.strip_prefix("topn ") {
        execute_collect(app, &format!("topn {}", sub.trim()));
    } else if let Some(sub) = cmd.strip_prefix("unique ") {
        execute_collect(app, &format!("unique {}", sub.trim()));
    } else if let Some(sub) = cmd.strip_prefix("numstats ") {
        execute_collect(app, &format!("numstats {}", sub.trim()));
    } else if cmd == "linestats" {
        execute_collect(app, "linestats");
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
    } else if let Some(path) = cmd.strip_prefix("append ") {
        let path = path.trim();
        let append_result = {
            let mut repo_mut = app.repo.borrow_mut();
            if let Some(ref mut r) = *repo_mut {
                r.append_file(Path::new(path)).map(|n| n)
            } else {
                Err(lga_core::error::LogAnalyzerError::Repo(
                    "No repo open".to_string(),
                ))
            }
        };
        match append_result {
            Ok(added) => {
                app.status_message =
                    format!("Appended {} lines from {}", added, path);
                app.refresh_line_count();
                app.load_viewport();
                app.re_search();
            }
            Err(e) => {
                app.error_message = Some(format!("Append failed: {}", e));
            }
        }
    } else if let Some(args) = cmd.strip_prefix("insert ") {
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        if parts.len() != 2 {
            app.error_message = Some("Usage: :insert <line_number> <text>".to_string());
        } else if let Ok(pos) = parts[0].parse::<usize>() {
            let after_line = pos.saturating_sub(1); // 1-based to 0-based
            app.queue_operation(Operation::InsertLines {
                after_line,
                content: vec![parts[1].to_string()],
            });
            app.status_message = format!("Insert line after {}", pos);
        } else {
            app.error_message = Some("Usage: :insert <line_number> <text>".to_string());
        }
    } else if let Some(args) = cmd.strip_prefix("modify ") {
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        if parts.len() != 2 {
            app.error_message = Some("Usage: :modify <line_number> <text>".to_string());
        } else if let Ok(idx) = parts[0].parse::<usize>() {
            let line_index = idx.saturating_sub(1); // 1-based to 0-based
            app.queue_operation(Operation::ModifyLine {
                line_index,
                new_content: parts[1].to_string(),
            });
            app.status_message = format!("Modified line {}", idx);
        } else {
            app.error_message = Some("Usage: :modify <line_number> <text>".to_string());
        }
    } else if let Some(args) = cmd.strip_prefix("merge ") {
        // Format: :merge src1 src2 -> target
        let parts: Vec<&str> = args.split("->").collect();
        if parts.len() != 2 {
            app.error_message =
                Some("Usage: :merge <src1> <src2> -> <target>".to_string());
        } else {
            let sources: Vec<&str> = parts[0].split_whitespace().collect();
            let target = parts[1].trim();
            if sources.is_empty() || target.is_empty() {
                app.error_message =
                    Some("Usage: :merge <src1> <src2> -> <target>".to_string());
            } else {
                match app.workspace.merge_repos(&sources, target) {
                    Ok(merged) => {
                        app.status_message = format!(
                            "Merged {} repos into '{}' ({} lines)",
                            sources.len(),
                            target,
                            merged.original_line_count()
                        );
                    }
                    Err(e) => {
                        app.error_message =
                            Some(format!("Merge failed: {}", e));
                    }
                }
            }
        }
    } else if let Some(name) = cmd.strip_prefix("branch del ") {
        let name = name.trim();
        let mut repo_mut = app.repo.borrow_mut();
        if let Some(ref mut r) = *repo_mut {
            match r.delete_branch(name) {
                Ok(true) => {
                    app.status_message = format!("Deleted branch '{}'", name);
                    drop(repo_mut);
                    app.build_history();
                }
                Ok(false) => {
                    app.error_message = Some(format!("Branch '{}' not found", name));
                }
                Err(e) => {
                    app.error_message = Some(format!("Cannot delete branch: {}", e));
                }
            }
        } else {
            app.error_message = Some("No repo open".to_string());
        }
        let name = name.trim();
        if name.is_empty() {
            app.error_message = Some("Usage: :branch <name>".to_string());
            return;
        }
        let mut repo_mut = app.repo.borrow_mut();
        if let Some(ref mut r) = *repo_mut {
            let head = r.head_node_id();
            match r.create_branch(name, head) {
                Ok(true) => {
                    app.status_message = format!("Created branch '{}' at node {}", name, head);
                }
                Ok(false) => {
                    app.error_message = Some(format!("Branch '{}' already exists", name));
                }
                Err(e) => {
                    app.error_message = Some(format!("Failed to create branch: {}", e));
                }
            }
        } else {
            app.error_message = Some("No repo open".to_string());
        }
    } else if let Some(name) = cmd.strip_prefix("checkout ") {
        let name = name.trim().to_string();
        if name.is_empty() {
            app.error_message = Some("Usage: :checkout <branch>".to_string());
            return;
        }
        let checkout_err: Option<String> = {
            let mut repo_mut = app.repo.borrow_mut();
            if let Some(ref mut r) = *repo_mut {
                match r.checkout_branch(&name) {
                    Ok(()) => None,
                    Err(e) => Some(format!("{}", e)),
                }
            } else {
                Some("No repo open".to_string())
            }
        };
        match checkout_err {
            None => {
                app.viewed_node_id = None;
                app.detached_head = false;
                app.status_message = format!("Switched to branch '{}'", name);
                app.refresh_line_count();
                app.load_viewport();
                app.re_search();
            }
            Some(err) => {
                app.error_message = Some(format!("Checkout failed: {}", err));
            }
        }
    } else if cmd == "branches" {
        let repo_ref = app.repo.borrow();
        if let Some(ref r) = *repo_ref {
            let names = r.branch_names();
            let current = r.current_branch();
            let list: Vec<String> = names
                .iter()
                .map(|n| {
                    if *n == current {
                        format!("* {}", n)
                    } else {
                        format!("  {}", n)
                    }
                })
                .collect();
            app.status_message = format!("Branches: {}", list.join(", "));
        } else {
            app.error_message = Some("No repo open".to_string());
        }
    } else if let Some(args) = cmd.strip_prefix("cache ") {
        let args = args.trim();
        if args == "stats" {
            let stats = app.cache_manager.stats();
            if stats.max_size_bytes > 0 {
                app.status_message = format!(
                    "Cache: {}/{} ({:.1}%) | {} entries | {} pinned",
                    format_bytes(stats.total_size_bytes),
                    format_bytes(stats.max_size_bytes),
                    if stats.max_size_bytes > 0 {
                        stats.total_size_bytes as f64 / stats.max_size_bytes as f64 * 100.0
                    } else {
                        0.0
                    },
                    stats.entry_count,
                    stats.pinned_count,
                );
            } else {
                app.status_message = format!(
                    "Cache: {} | {} entries | {} pinned (no size limit)",
                    format_bytes(stats.total_size_bytes),
                    stats.entry_count,
                    stats.pinned_count,
                );
            }
        } else if let Some(mb_str) = args.strip_prefix("max ") {
            if let Ok(mb) = mb_str.trim().parse::<u64>() {
                app.cache_manager.set_session_max_mb(mb);
                app.status_message = format!("Cache max size set to {} MB (session)", mb);
            } else {
                app.error_message = Some("Usage: :cache max <mb>".to_string());
            }
        } else if let Some(node_str) = args.strip_prefix("pin ") {
            if let Ok(node_id) = node_str.trim().parse::<usize>() {
                app.cache_manager.pin(node_id);
                app.status_message = format!("Pinned node {} (never evicted)", node_id);
            } else {
                app.error_message = Some("Usage: :cache pin <node_id>".to_string());
            }
        } else if let Some(node_str) = args.strip_prefix("unpin ") {
            if let Ok(node_id) = node_str.trim().parse::<usize>() {
                app.cache_manager.unpin(node_id);
                app.status_message = format!("Unpinned node {}", node_id);
            } else {
                app.error_message = Some("Usage: :cache unpin <node_id>".to_string());
            }
        } else {
            app.error_message = Some(
                "Usage: :cache stats|max <mb>|pin <nid>|unpin <nid>".to_string(),
            );
        }
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

/// Parse and execute a collect sub-command.
///
/// Supported forms:
///   count [pattern]
///   group <pattern> <group_index>
///   topn <pattern> <group_index> <n>
///   unique <pattern> <group_index>
///   numstats <pattern> <group_index>
///   linestats
fn execute_collect(app: &mut App, sub: &str) {
    let parts: Vec<&str> = sub.splitn(2, ' ').collect();
    let kind = parts[0];
    let args = parts.get(1).unwrap_or(&"").trim();

    let collector = match kind {
        "count" => {
            let pattern = if args.is_empty() { None } else { Some(args.to_string()) };
            Some(Collector::Count { pattern })
        }
        "group" => {
            let mut p = args.splitn(2, ' ');
            let pattern = p.next().unwrap_or("").to_string();
            let group_index: usize = p.next().unwrap_or("1").parse().unwrap_or(1);
            if pattern.is_empty() {
                app.error_message = Some("Usage: group <pattern> <group_index>".to_string());
                return;
            }
            Some(Collector::GroupCount { pattern, group_index })
        }
        "topn" => {
            let mut p = args.splitn(3, ' ');
            let pattern = p.next().unwrap_or("").to_string();
            let group_index: usize = p.next().unwrap_or("1").parse().unwrap_or(1);
            let n: usize = p.next().unwrap_or("10").parse().unwrap_or(10);
            if pattern.is_empty() {
                app.error_message = Some("Usage: topn <pattern> <group_index> <n>".to_string());
                return;
            }
            Some(Collector::TopN { pattern, group_index, n })
        }
        "unique" => {
            let mut p = args.splitn(2, ' ');
            let pattern = p.next().unwrap_or("").to_string();
            let group_index: usize = p.next().unwrap_or("1").parse().unwrap_or(1);
            if pattern.is_empty() {
                app.error_message = Some("Usage: unique <pattern> <group_index>".to_string());
                return;
            }
            Some(Collector::Unique { pattern, group_index })
        }
        "numstats" => {
            let mut p = args.splitn(2, ' ');
            let pattern = p.next().unwrap_or("").to_string();
            let group_index: usize = p.next().unwrap_or("1").parse().unwrap_or(1);
            if pattern.is_empty() {
                app.error_message = Some("Usage: numstats <pattern> <group_index>".to_string());
                return;
            }
            Some(Collector::NumericStats { pattern, group_index })
        }
        "linestats" => {
            Some(Collector::LineStats)
        }
        _ => {
            app.error_message = Some(format!(
                "Unknown collect type: {}. Use: count, group, topn, unique, numstats, linestats",
                kind
            ));
            return;
        }
    };

    if let Some(c) = collector {
        app.run_collect(c);
    }
}

/// Resolve a pattern that may be a @name reference to a saved filter.
fn resolve_pattern(config: &lga_core::config::Config, pattern: &str) -> String {
    let trimmed = pattern.trim();
    if let Some(name) = trimmed.strip_prefix('@') {
        config.get_filter(name).map(|s| s.to_string()).unwrap_or_else(|| {
            trimmed.to_string()
        })
    } else {
        trimmed.to_string()
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", size, UNITS[unit])
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
    } else if prompt.starts_with("Append file path") {
        let path = Path::new(input);
        if !path.exists() {
            app.error_message = Some(format!("File not found: {}", input));
            return;
        }
        let append_result = {
            let mut repo_mut = app.repo.borrow_mut();
            if let Some(ref mut r) = *repo_mut {
                r.append_file(path).map(|n| n)
            } else {
                Err(lga_core::error::LogAnalyzerError::Repo(
                    "No repo open".to_string(),
                ))
            }
        };
        match append_result {
            Ok(added) => {
                app.status_message =
                    format!("Appended {} lines from {}", added, input);
                app.refresh_line_count();
                app.load_viewport();
                app.re_search();
            }
            Err(e) => {
                app.error_message = Some(format!("Append failed: {}", e));
            }
        }
    } else if prompt.starts_with("Insert after line") {
        let pos: usize = prompt
            .strip_prefix("Insert after line ")
            .and_then(|s| s.strip_suffix(": "))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0); // 1-based
        let after_line = pos.saturating_sub(1);
        app.queue_operation(Operation::InsertLines {
            after_line,
            content: vec![input.to_string()],
        });
        app.status_message = format!("Inserted line after {}", pos);
    } else if prompt.starts_with("Modify line") {
        let line_index: usize = prompt
            .strip_prefix("Modify line ")
            .and_then(|s| s.strip_suffix(": "))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0)
            .saturating_sub(1); // 1-based to 0-based
        app.queue_operation(Operation::ModifyLine {
            line_index,
            new_content: input.to_string(),
        });
        app.status_message = format!("Modified line {}", line_index + 1);
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
