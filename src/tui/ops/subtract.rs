//! Execute [`PendingOp::SubtractNodes`] — diff-subtract one node from another.

use crate::tui::app::App;

pub fn execute(app: &mut App, base: usize, subtrahend: usize, branch: String) {
    let repo_hash = {
        let repo_mut = app.repo.borrow_mut();
        repo_mut
            .as_ref()
            .map(|r| lograph::cache::hash_repo_path(r.path()))
    };
    let mut repo_mut = app.repo.borrow_mut();
    if let Some(ref mut r) = *repo_mut {
        match r.subtract_nodes(base, subtrahend, &branch) {
            Ok(new_id) => {
                app.status_message = format!(
                    "Subtracted node {} from node {} → new node {} on '{}'",
                    subtrahend, base, new_id, branch
                );
                app.line_count_is_original = false;
                app.viewed_node_id = None;
                app.detached_head = false;
            }
            Err(e) => {
                app.error_message = Some(format!("Subtract failed: {}", e));
            }
        }
    }
    app.diff_base_node_id = None;
    drop(repo_mut);
    if let Some(ref h) = repo_hash {
        app.cache_manager.clear_repo(h);
    }
    app.build_history();
    app.refresh_line_count();
    app.load_viewport();
    app.clear_search();
}
