//! Execute [`PendingOp::ApplyOperationFrom`] — branch off a historical node.

use lograph::operator::Operation;

use crate::tui::app::App;

pub fn execute(app: &mut App, from_node_id: usize, op: Operation) {
    let desc = op.describe();
    let is_collect = matches!(op, Operation::Collect { .. });
    let mut repo_mut = app.repo.borrow_mut();
    if let Some(ref mut r) = *repo_mut {
        let branch_name = match &op {
            Operation::Filter { pattern, keep } => {
                if *keep {
                    format!("filter-{}", pattern)
                } else {
                    format!("filter-rm-{}", pattern)
                }
            }
            Operation::Replace { pattern, .. } => {
                format!("replace-{}", pattern)
            }
            Operation::Collect { collector } => {
                format!("collect-{}", collector.describe().replace(' ', "-"))
            }
            _ => String::from("branch"),
        };
        match r.apply_operation_from(from_node_id, &branch_name, op) {
            Ok(()) => {
                app.status_message = format!(
                    "Created branch '{}' and applied: {}",
                    branch_name, desc
                );
                app.line_count_is_original = false;
                app.viewed_node_id = None;
                app.detached_head = false;
                if is_collect {
                    if let Some(summary) = app.pending_collect_summary.take() {
                        let new_head = r.head_node_id();
                        app.collect_results.insert(new_head, summary);
                    }
                    app.tag_store.repos.remove(&app.repo_name);
                    let _ = app.tag_store.save(&app.workspace.root());
                }
            }
            Err(e) => {
                app.error_message = Some(format!("Branch operation failed: {}", e));
            }
        }
    }
    drop(repo_mut);
    let repo_hash = {
        let repo_ref = app.repo.borrow();
        repo_ref.as_ref().map(|r| lograph::cache::hash_repo_path(r.path()))
    };
    if let Some(ref h) = repo_hash {
        app.cache_manager.clear_repo(h);
    }
    app.build_history();
    app.refresh_line_count();
    app.load_viewport();
    app.clear_search();
}
