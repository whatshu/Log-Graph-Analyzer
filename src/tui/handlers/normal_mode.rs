use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lograph::operator::Operation;

use crate::tui::app::{App, InputMode, PendingOp, ViewKind};
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
                ViewKind::RepoList => {
                    let repos = app.workspace.list().unwrap_or_default();
                    if app.repo_cursor + 1 < repos.len() {
                        app.repo_cursor += 1;
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
                ViewKind::RepoList => {
                    if app.repo_cursor > 0 {
                        app.repo_cursor -= 1;
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
            } else if app.active_view == ViewKind::RepoList {
                app.repo_cursor = 0;
            } else {
                app.go_to_line(0);
            }
        }
        KeyCode::Char('G') => {
            if app.active_view == ViewKind::History {
                app.history_cursor = app.history_nodes.len().saturating_sub(1);
            } else if app.active_view == ViewKind::RepoList {
                let repos = app.workspace.list().unwrap_or_default();
                app.repo_cursor = repos.len().saturating_sub(1);
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
        // c: enter collect command mode, or clone repo in RepoList
        KeyCode::Char('c') => {
            if app.active_view == ViewKind::RepoList {
                let repos = app.workspace.list().unwrap_or_default();
                if app.repo_cursor < repos.len() {
                    let src = repos[app.repo_cursor].clone();
                    app.input_mode = InputMode::Input;
                    app.input_buffer.clear();
                    app.input_prompt = format!("Clone '{}' as (new name): ", src);
                    app.pending_repo_clone_src = Some(src);
                }
            } else {
                app.input_mode = InputMode::Command;
                app.input_buffer = String::from("collect ");
                app.input_prompt = String::from(":");
            }
        }
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
        // H: return to HEAD from view mode (like Shift+H)
        KeyCode::Char('H') => {
            app.return_to_head();
            app.active_view = ViewKind::LogView;
            app.load_viewport();
        }
        KeyCode::Char('r') => {
            // Clamp repo_cursor when entering RepoList view
            let repos = app.workspace.list().unwrap_or_default();
            if app.repo_cursor >= repos.len() {
                app.repo_cursor = repos.len().saturating_sub(1);
            }
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
                let current = app.repo.borrow().as_ref().and_then(|_r| {
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

        // History view specific — Enter checks out a node and shows its state
        KeyCode::Enter => {
            if app.active_view == ViewKind::History {
                if app.history_cursor < app.history_nodes.len() {
                    let node_id = app.history_nodes[app.history_cursor].id;
                    app.queue_checkout(node_id);
                    app.horizontal_scroll = 0;
                    app.active_view = ViewKind::LogView;
                }
            } else if app.active_view == ViewKind::RepoList {
                if let Ok(repos) = app.workspace.list() {
                    if !repos.is_empty() {
                        let idx = app.repo_cursor.min(repos.len().saturating_sub(1));
                        app.open_repo(Some(&repos[idx]));
                    }
                }
            }
        }
        // ── Tag system ──
        KeyCode::Char('t') => {
            app.show_tag_manager = !app.show_tag_manager;
            app.tag_manager_cursor = 0;
            app.tag_manager_scroll = 0;
            app.tag_manager_h_scroll = 0;
            if app.show_tag_manager {
                app.status_message = String::from("Tag Manager — j/k:nav  Enter:jump  d:del  r:rename  y:copy  q:close");
            }
        }

        // ── History node operations (in History view) ──
        KeyCode::Char(' ') => {
            if app.active_view == ViewKind::History && app.history_cursor < app.history_nodes.len() {
                let node_id = app.history_nodes[app.history_cursor].id;
                if app.history_marks.contains(&node_id) {
                    app.history_marks.remove(&node_id);
                    app.status_message = format!("Unmarked node {}", node_id);
                } else {
                    app.history_marks.insert(node_id);
                    app.status_message = format!("Marked node {} ({} total)", node_id, app.history_marks.len());
                }
            }
        }
        KeyCode::Char('m') => {
            if app.active_view == ViewKind::History {
                if app.history_marks.len() < 2 {
                    app.error_message = Some("Need at least 2 marked nodes to merge. Use Space to mark.".into());
                } else {
                    // Sort sources for deterministic order
                    let mut sources: Vec<usize> =
                        app.history_marks.iter().copied().collect();
                    sources.sort_unstable();
                    app.merge_sources = sources;
                    app.merge_mode_cursor = 0; // Default: OR (Union)
                    app.show_merge_mode_popup = true;
                }
            }
        }
        KeyCode::Char('d') => {
            if app.active_view == ViewKind::RepoList {
                let repos = app.workspace.list().unwrap_or_default();
                if app.repo_cursor < repos.len() {
                    let name = repos[app.repo_cursor].clone();
                    // If deleting the currently open repo, close it first
                    if name == app.repo_name {
                        *app.repo.borrow_mut() = None;
                        app.repo_name.clear();
                        app.viewport_lines.clear();
                        app.total_lines = 0;
                    }
                    match app.workspace.remove_repo(&name) {
                        Ok(()) => {
                            app.status_message = format!("Deleted repo '{}'", name);
                            // Clamp cursor after deletion
                            let new_repos = app.workspace.list().unwrap_or_default();
                            if app.repo_cursor >= new_repos.len() {
                                app.repo_cursor = new_repos.len().saturating_sub(1);
                            }
                        }
                        Err(e) => app.error_message = Some(format!("Delete failed: {}", e)),
                    }
                }
            } else if app.active_view == ViewKind::History && app.history_cursor < app.history_nodes.len() {
                let node_id = app.history_nodes[app.history_cursor].id;
                if let Some(base) = app.diff_base_node_id.take() {
                    if base == node_id {
                        app.error_message = Some("Cannot diff a node with itself".into());
                    } else {
                        let branch = format!("diff-{}-{}", base, node_id);
                        app.pending_op = PendingOp::SubtractNodes { base, subtrahend: node_id, branch };
                        app.status_message = format!("Subtracting node {} from node {}", node_id, base);
                    }
                } else {
                    app.diff_base_node_id = Some(node_id);
                    app.status_message = format!("Diff base set to node {}. Press d on another node to subtract.", node_id);
                }
            }
        }
        KeyCode::Char('y') => {
            if app.active_view == ViewKind::History && app.history_cursor < app.history_nodes.len() {
                let node_id = app.history_nodes[app.history_cursor].id;
                app.yanked_node_id = Some(node_id);
                app.status_message = format!("Yanked node {} — press p to paste", node_id);
            }
        }
        KeyCode::Char('p') => {
            if app.active_view == ViewKind::History {
                if let Some(src_id) = app.yanked_node_id {
                    if app.history_cursor < app.history_nodes.len() {
                        let parent_id = app.history_nodes[app.history_cursor].id;
                        let branch = format!("replay-{}", src_id);
                        app.pending_op = PendingOp::ReplayNode { source: src_id, target_parent: parent_id, branch };
                        app.status_message = format!("Replaying node {} at node {}", src_id, parent_id);
                    }
                } else {
                    app.error_message = Some("Nothing yanked — press y on a node first".into());
                }
            }
        }
        KeyCode::Char('D') => {
            if app.active_view == ViewKind::History && app.history_cursor < app.history_nodes.len() {
                let node_id = app.history_nodes[app.history_cursor].id;
                if node_id == 0 {
                    app.error_message = Some("Cannot delete root node".into());
                } else {
                    app.pending_op = PendingOp::SoftDelete { node_id };
                    app.status_message = format!("Soft-deleting node {}...", node_id);
                }
            }
        }
        _ => {}
    }
}
