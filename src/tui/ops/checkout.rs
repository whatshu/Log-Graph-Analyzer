//! Execute [`PendingOp::CheckoutTo`] — view a historical node state.

use crate::tui::app::App;

pub fn execute(app: &mut App, node_idx: usize) {
    let node_id = node_idx;
    let is_detached = {
        let repo_ref = app.repo.borrow();
        repo_ref.as_ref().map_or(true, |r| node_id != r.head_node_id())
    };
    match app.get_node_lines(node_id) {
        Ok(lines) => {
            app.viewed_node_id = Some(node_id);
            app.detached_head = is_detached;
            app.status_message = format!(
                "Viewing node {} ({}). Apply operation to branch off.",
                node_id,
                if is_detached { "detached" } else { "HEAD" }
            );
            app.total_lines = lines.len();
            app.line_count_is_original = node_id == 0;
        }
        Err(e) => {
            app.error_message = Some(format!("View node failed: {}", e));
            return;
        }
    }
    app.load_viewport();
    app.clear_search();
    app.build_history();
}
