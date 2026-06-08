use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use log_analyzer_core::operator::Operation;

use super::app::{App, InputMode, ViewKind};

pub fn normal_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.scroll_down(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.scroll_up(1);
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.page_down();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.page_up();
        }
        KeyCode::Char('g') => {
            app.go_to_line(0);
        }
        KeyCode::Char('G') => {
            if app.total_lines > 0 {
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
        KeyCode::Char('n') => {
            app.next_match();
        }
        KeyCode::Char('N') => {
            app.prev_match();
        }
        KeyCode::Char('u') => {
            app.queue_undo();
        }
        KeyCode::Char('r') => {
            app.active_view = ViewKind::RepoList;
            app.status_message = String::from("Repo list");
        }
        KeyCode::Char('l') => {
            app.active_view = ViewKind::LogView;
            app.load_viewport();
        }
        KeyCode::Char('a') => {
            app.active_view = ViewKind::Analytics;
        }
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
        }
        KeyCode::Char('i') => {
            app.input_mode = InputMode::Input;
            app.input_buffer.clear();
            app.input_prompt = String::from("Import file path: ");
        }
        KeyCode::Char('e') => {
            app.input_mode = InputMode::Input;
            app.input_buffer.clear();
            app.input_prompt = String::from("Export to file: ");
        }
        KeyCode::Enter => {
            if app.active_view == ViewKind::RepoList {
                if let Ok(repos) = app.workspace.list() {
                    if !repos.is_empty() {
                        app.open_repo(Some(&repos[0]));
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
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        _ => {}
    }
}

pub fn search_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let query = app.input_buffer.clone();
            app.input_buffer.clear();
            app.input_mode = InputMode::Normal;
            if !query.is_empty() {
                app.do_search(&query);
            }
        }
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => {
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
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        _ => {}
    }
}

fn execute_command(app: &mut App, cmd: &str) {
    let cmd = cmd.trim();

    if cmd.is_empty() {
        return;
    }

    if cmd == "q" {
        app.should_quit = true;
    } else if let Some(pattern) = cmd.strip_prefix("f ") {
        app.queue_operation(Operation::Filter {
            pattern: pattern.to_string(),
            keep: true,
        });
        app.status_message = format!("Filter keep: {}", pattern);
    } else if let Some(pattern) = cmd.strip_prefix("fr ") {
        app.queue_operation(Operation::Filter {
            pattern: pattern.to_string(),
            keep: false,
        });
        app.status_message = format!("Filter remove: {}", pattern);
    } else if let Some(args) = cmd.strip_prefix("r ") {
        if let Some(inner) = parse_delimited(args, '/') {
            let parts: Vec<&str> = inner.splitn(2, '/').collect();
            if parts.len() == 2 {
                app.queue_operation(Operation::Replace {
                    pattern: parts[0].to_string(),
                    replacement: parts[1].to_string(),
                });
                app.status_message =
                    format!("Replace /{}/ -> {}", parts[0], parts[1]);
            } else {
                app.error_message =
                    Some("Invalid replace syntax. Use :r /pat/repl/".to_string());
            }
        } else {
            app.error_message =
                Some("Invalid replace syntax. Use :r /pat/repl/".to_string());
        }
    } else if let Some(path) = cmd.strip_prefix("w ") {
        let path = path.trim();
        let mut repo_mut = app.repo.borrow_mut();
        if let Some(ref mut r) = *repo_mut {
            match r.export(std::path::Path::new(path)) {
                Ok(()) => {
                    app.status_message = format!("Exported to {}", path);
                }
                Err(e) => {
                    app.error_message = Some(format!("Export failed: {}", e));
                }
            }
        } else {
            app.error_message = Some("No repo open".to_string());
        }
    } else if let Some(name) = cmd.strip_prefix("repo ") {
        let name = name.trim();
        app.open_repo(Some(name));
    } else if cmd == "stats" {
        app.active_view = ViewKind::Analytics;
    } else {
        app.error_message = Some(format!("Unknown command: {}", cmd));
    }
}

fn parse_delimited<'a>(s: &'a str, delim: char) -> Option<&'a str> {
    let s = s.strip_prefix(delim)?;
    let end = s.find(delim)?;
    Some(&s[..end])
}

fn handle_input(app: &mut App, prompt: &str, input: &str) {
    if prompt.contains("Import") {
        let path = std::path::Path::new(input);
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
            Err(e) => {
                app.error_message = Some(format!("Import failed: {}", e));
            }
        }
    } else if prompt.contains("Export") {
        let mut repo_mut = app.repo.borrow_mut();
        if let Some(ref mut r) = *repo_mut {
            match r.export(std::path::Path::new(input)) {
                Ok(()) => {
                    app.status_message = format!("Exported to {}", input);
                }
                Err(e) => {
                    app.error_message = Some(format!("Export failed: {}", e));
                }
            }
        } else {
            app.error_message = Some("No repo open".to_string());
        }
    }
}
