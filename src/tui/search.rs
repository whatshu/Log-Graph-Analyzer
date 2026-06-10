//! Search functionality — regex search, match navigation, and search state management.

use lograph::repo::LogRepo;

use crate::tui::app::App;

/// Execute a regex search against the current log data.
/// Stores results in `app.search_results` and navigates to the first match.
pub fn do_search(app: &mut App, query: &str) {
    app.search_query = query.to_string();
    app.search_results.clear();
    app.search_index = 0;

    let results: Vec<usize> = {
        if let Some(node_id) = app.viewed_node_id {
            let repo_ref = app.repo.borrow();
            if let Some(ref r) = *repo_ref {
                let lines = r.view_node(node_id).unwrap_or_default();
                match regex::Regex::new(query) {
                    Ok(re) => lines
                        .iter()
                        .enumerate()
                        .filter(|(_, line)| re.is_match(line))
                        .take(10_000)
                        .map(|(i, _)| i)
                        .collect(),
                    Err(_) => Vec::new(),
                }
            } else {
                Vec::new()
            }
        } else {
            let repo_ref = app.repo.borrow();
            if repo_ref.is_none() {
                return;
            }

            let has_ops = repo_ref
                .as_ref()
                .map_or(false, |r: &LogRepo| !r.history_tree().is_empty());
            if has_ops {
                drop(repo_ref);
                let mut repo_mut = app.repo.borrow_mut();
                if let Some(ref mut r) = *repo_mut {
                    let lines = r.get_current_lines().unwrap_or_default();
                    match regex::Regex::new(query) {
                        Ok(re) => lines
                            .iter()
                            .enumerate()
                            .filter(|(_, line)| re.is_match(line))
                            .take(10_000)
                            .map(|(i, _)| i)
                            .collect(),
                        Err(_) => Vec::new(),
                    }
                } else {
                    Vec::new()
                }
            } else {
                let r = repo_ref.as_ref().unwrap();
                let proc = r.processor();
                proc.parallel_search(query, 10_000)
                    .unwrap_or_default()
                    .iter()
                    .map(|(idx, _)| *idx)
                    .collect()
            }
        }
    };

    app.search_results = results;
    if !app.search_results.is_empty() {
        app.search_index = 0;
        let target = app.search_results[0];
        app.cursor_line = target;
        if target < app.scroll_offset || target >= app.scroll_offset + 50 {
            app.scroll_offset = target.saturating_sub(5);
        }
        app.load_viewport();
        app.status_message = format!(
            "Match {}/{}",
            app.search_index + 1,
            app.search_results.len()
        );
    } else {
        app.status_message = String::from("No matches found");
    }
}

/// Clear the current search state (query, results, index).
/// Called after operations that change the data so highlights don't
/// persist onto the new dataset.
pub fn clear_search(app: &mut App) {
    app.search_query.clear();
    app.search_results.clear();
    app.search_index = 0;
}

/// Jump to the next match.
pub fn next_match(app: &mut App) {
    if app.search_results.is_empty() {
        return;
    }
    app.search_index = (app.search_index + 1) % app.search_results.len();
    jump_to_match(app);
}

/// Jump to the previous match.
pub fn prev_match(app: &mut App) {
    if app.search_results.is_empty() {
        return;
    }
    app.search_index = if app.search_index == 0 {
        app.search_results.len() - 1
    } else {
        app.search_index - 1
    };
    jump_to_match(app);
}

/// Jump to the current search match position.
pub(crate) fn jump_to_match(app: &mut App) {
    let target = app.search_results[app.search_index];
    app.cursor_line = target;
    if target < app.scroll_offset || target >= app.scroll_offset + 50 {
        app.scroll_offset = target.saturating_sub(5);
    }
    app.load_viewport();
    app.status_message = format!(
        "Match {}/{}",
        app.search_index + 1,
        app.search_results.len()
    );
}
