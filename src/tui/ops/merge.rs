//! Execute [`PendingOp::MergeNodes`] — merge multiple history nodes.

use lograph::operator::MergeMode;

use crate::tui::app::App;

pub fn execute(app: &mut App, sources: Vec<usize>, branch: String, mode: MergeMode) {
    let repo_hash = {
        let repo_mut = app.repo.borrow_mut();
        repo_mut
            .as_ref()
            .map(|r| lograph::cache::hash_repo_path(r.path()))
    };
    let mut repo_mut = app.repo.borrow_mut();
    if let Some(ref mut r) = *repo_mut {
        match r.merge_nodes(&sources, &branch, mode) {
            Ok(new_id) => {
                let mode_str = match mode {
                    MergeMode::Union => "OR",
                    MergeMode::Intersection => "AND",
                    MergeMode::Subtract => "SUB",
                    MergeMode::Xor => "XOR",
                };
                app.status_message = format!(
                    "Merged {} nodes ({}) into new node {} on branch '{}'",
                    sources.len(),
                    mode_str,
                    new_id,
                    branch
                );
                app.line_count_is_original = false;
                app.viewed_node_id = None;
                app.detached_head = false;
            }
            Err(e) => {
                app.error_message = Some(format!("Merge failed: {}", e));
            }
        }
    }
    app.history_marks.clear();
    drop(repo_mut);
    if let Some(ref h) = repo_hash {
        app.cache_manager.clear_repo(h);
    }
    app.build_history();
    app.refresh_line_count();
    app.load_viewport();
    app.clear_search();
}
