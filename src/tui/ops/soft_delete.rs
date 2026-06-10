//! Execute [`PendingOp::SoftDelete`] — soft-delete a history node.

use crate::tui::app::App;

pub fn execute(app: &mut App, node_id: usize) {
    let mut repo_mut = app.repo.borrow_mut();
    if let Some(ref mut r) = *repo_mut {
        match r.soft_delete_node(node_id) {
            Ok(count) => {
                if count == 1 {
                    app.status_message = format!("Soft-deleted node {}", node_id);
                } else {
                    app.status_message = format!(
                        "Soft-deleted node {} and {} cascade node(s)",
                        node_id,
                        count - 1
                    );
                }
            }
            Err(e) => {
                app.error_message = Some(format!("Delete failed: {}", e));
            }
        }
    }
    drop(repo_mut);
    app.build_history();
    app.refresh_line_count();
    app.load_viewport();
    app.clear_search();
}
