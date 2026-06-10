//! Execute [`PendingOp::ApplyOperation`] — apply an operation at HEAD.

use lograph::operator::Operation;

use crate::tui::app::App;

pub fn execute(app: &mut App, op: Operation) {
    let desc = op.describe();
    let is_collect = matches!(op, Operation::Collect { .. });

    // Snapshot old lines for tag remapping (non-collect only)
    let old_lines: Option<Vec<String>> = if !is_collect {
        let mut repo_mut = app.repo.borrow_mut();
        repo_mut.as_mut().and_then(|r| {
            if r.history_tree().is_empty() {
                r.read_all_original_lines().ok()
            } else {
                r.get_current_lines().ok()
            }
        })
    } else {
        None
    };

    let mut repo_mut = app.repo.borrow_mut();
    if let Some(ref mut r) = *repo_mut {
        match r.apply_operation(op.clone()) {
            Ok(()) => {
                app.status_message = format!("Applied: {}", desc);
                app.line_count_is_original = false;
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
                app.error_message = Some(format!("Operation failed: {}", e));
            }
        }
    }
    drop(repo_mut);

    if !is_collect {
        if let Some(ref old) = old_lines {
            app.remap_tags_after_operation(&op, old);
        }
    }

    app.build_history();
    app.refresh_line_count();
    app.load_viewport();
    app.clear_search();
}
