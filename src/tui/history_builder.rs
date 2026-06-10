//! History tree building and search history persistence.



use crate::tui::app::{App, HistoryNode};

// ── History tree building ──

/// Build the history view nodes from the tree topological order.
pub fn build_history(app: &mut App) {
    app.history_nodes.clear();
    let repo_ref = app.repo.borrow();
    if let Some(ref r) = *repo_ref {
        let tree = r.history_tree();
        let order = tree.topological_order();

        let head_id = tree.head();
        let viewed_id = app.viewed_node_id;

        for entry in &order {
            let desc = if entry.node_id == 0 {
                format!("Import — {} lines", r.original_line_count())
            } else {
                entry.description.clone()
            };
            let line_count = if entry.node_id == 0 {
                r.original_line_count()
            } else {
                r.line_count_at(entry.node_id).unwrap_or(0)
            };
            let is_head = entry.node_id == head_id || entry.is_current_head;
            let is_viewed = viewed_id == Some(entry.node_id);

            let connector = build_connector(
                entry.depth,
                &entry.ancestors,
                entry.sibling_count,
                entry.is_last_child,
                app.ascii_only,
            );

            let collect_summary = app.collect_results.get(&entry.node_id).cloned();

            app.history_nodes.push(HistoryNode {
                id: entry.node_id,
                description: desc,
                line_count,
                applied_at: if entry.node_id == 0 {
                    String::from("—")
                } else {
                    entry.applied_at.format("%Y-%m-%d %H:%M").to_string()
                },
                connector,
                depth: entry.depth,
                branch_labels: entry.branch_labels.clone(),
                is_head,
                is_viewed,
                collect_summary,
                deleted: entry.deleted,
                tag_name: entry.tag_name.clone(),
                is_last_child: entry.is_last_child,
                sibling_count: entry.sibling_count,
            });
        }
    }
    // Set cursor to HEAD node
    let head_id = {
        let repo_ref = app.repo.borrow();
        repo_ref.as_ref().map(|r| r.head_node_id()).unwrap_or(0)
    };
    app.history_cursor = app
        .history_nodes
        .iter()
        .position(|n| n.id == head_id)
        .unwrap_or(app.history_nodes.len().saturating_sub(1));
}

/// Build a tree connector string like git log --graph.
/// continuing_forks: at each display-depth level, whether the fork at that level
///   still has more children to show (controls vertical `│` lines).
/// sibling_count: number of children the parent node has. 1 = only child (linear), >1 = fork.
/// is_last_child: whether this is the last child of its parent.
/// When `ascii_only` is true, uses ASCII characters (|, `--, `--) instead of
/// Unicode box-drawing (│, ├─, └─) for terminals that don't support UTF-8.
pub fn build_connector(
    depth: usize,
    continuing_forks: &[bool],
    sibling_count: usize,
    is_last_child: bool,
    ascii_only: bool,
) -> String {
    if depth == 0 {
        return String::new();
    }
    let mut s = String::with_capacity(depth * 2);
    for d in 0..depth - 1 {
        let has_continuing = continuing_forks.get(d).copied().unwrap_or(false);
        if has_continuing {
            if ascii_only {
                s.push_str("| ");
            } else {
                s.push_str("│ ");
            }
        } else {
            s.push_str("  ");
        }
    }
    if sibling_count > 1 {
        if is_last_child {
            if ascii_only {
                s.push_str("`-");
            } else {
                s.push_str("└─");
            }
        } else {
            if ascii_only {
                s.push_str("|-");
            } else {
                s.push_str("├─");
            }
        }
    } else {
        s.push_str("  ");
    }
    s
}

// ── Search history persistence ──

pub(crate) fn history_path() -> std::path::PathBuf {
    std::path::PathBuf::from(".log_analyzer").join("search_history.json")
}

pub(crate) fn load_search_history() -> Vec<String> {
    let path = history_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(&data) {
            return list.into_iter().take(100).collect();
        }
    }
    Vec::new()
}

pub fn save_search_history(app: &App) {
    let path = history_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string(&app.search_history).unwrap_or_default();
    let _ = std::fs::write(&path, json);
}

/// Add a search term to history (dedup, most recent first, capped at 100).
pub fn add_to_history(app: &mut App, term: &str) {
    let term = term.trim().to_string();
    if term.is_empty() {
        return;
    }
    app.search_history.retain(|t| t != &term);
    app.search_history.insert(0, term);
    app.search_history.truncate(100);
    save_search_history(app);
}

/// Step up/back in search history. Returns the term to fill in.
pub fn history_navigate_up(app: &mut App) -> Option<&str> {
    if app.search_history.is_empty() {
        return None;
    }
    let next = (app.search_history_idx + 1).min(app.search_history.len() as isize - 1);
    app.search_history_idx = next;
    app.search_history.get(next as usize).map(|s| s.as_str())
}

/// Step down/forward in search history. Returns the term or empty.
pub fn history_navigate_down(app: &mut App) -> Option<&str> {
    if app.search_history_idx <= 0 {
        app.search_history_idx = -1;
        return Some("");
    }
    app.search_history_idx -= 1;
    app.search_history
        .get(app.search_history_idx as usize)
        .map(|s| s.as_str())
}

/// Reset history navigation position.
pub fn history_reset(app: &mut App) {
    app.search_history_idx = -1;
}
