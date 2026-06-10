//! Execute [`PendingOp::Undo`] — undo the last operation.

use crate::tui::app::App;

pub fn execute(app: &mut App) {
    let repo_hash = {
        let repo_mut = app.repo.borrow_mut();
        repo_mut
            .as_ref()
            .map(|r| lograph::cache::hash_repo_path(r.path()))
    };
    // Snapshot tags before undo so we can restore them after
    let tags_before = app.tag_store.get_tags(&app.repo_name).to_vec();
    let mut repo_mut = app.repo.borrow_mut();
    if let Some(ref mut r) = *repo_mut {
        match r.undo() {
            Ok(op) => {
                app.status_message = format!("Undone: {}", op.describe());
                if r.history_tree().is_empty() {
                    app.line_count_is_original = true;
                }
            }
            Err(e) => {
                app.error_message = Some(format!("Undo failed: {}", e));
            }
        }
    }
    drop(repo_mut);
    // Restore tags (best-effort: ranges may have shifted, but we keep the tags)
    if !tags_before.is_empty() {
        app.tag_store
            .repos
            .insert(app.repo_name.clone(), tags_before);
        let _ = app.tag_store.save(&app.workspace.root());
    }
    if let Some(ref h) = repo_hash {
        app.cache_manager.clear_repo(h);
    }
    app.build_history();
    app.refresh_line_count();
    app.load_viewport();
    app.clear_search();
}
