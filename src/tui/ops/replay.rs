//! Execute [`PendingOp::ReplayNode`] — replay a node's operation at a different position.

use crate::tui::app::App;

pub fn execute(app: &mut App, source: usize, target_parent: usize, branch: String) {
    let repo_hash = {
        let repo_mut = app.repo.borrow_mut();
        repo_mut
            .as_ref()
            .map(|r| lograph::cache::hash_repo_path(r.path()))
    };
    let mut repo_mut = app.repo.borrow_mut();
    if let Some(ref mut r) = *repo_mut {
        match r.replay_node_at(source, target_parent, &branch) {
            Ok(new_id) => {
                app.status_message = format!(
                    "Replayed node {} at node {} → new node {} on '{}'",
                    source, target_parent, new_id, branch
                );
                app.line_count_is_original = false;
                app.viewed_node_id = None;
                app.detached_head = false;
            }
            Err(e) => {
                app.error_message = Some(format!("Replay failed: {}", e));
            }
        }
    }
    drop(repo_mut);
    if let Some(ref h) = repo_hash {
        app.cache_manager.clear_repo(h);
    }
    app.build_history();
    app.refresh_line_count();
    app.load_viewport();
    app.clear_search();
}
