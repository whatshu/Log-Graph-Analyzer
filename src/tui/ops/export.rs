//! Execute [`PendingOp::ExportFrom`] — export log state at a specific node.

use crate::tui::app::App;

pub fn execute(app: &mut App, node_idx: usize, path: String) {
    let repo_ref = app.repo.borrow();
    if let Some(ref r) = *repo_ref {
        let node_id = node_idx;
        let result = if node_id == 0 {
            r.read_all_original_lines().map(|lines| lines.join("\n"))
        } else {
            r.compute_state_at(node_id).map(|lines| lines.join("\n"))
        };
        match result {
            Ok(content) => {
                if let Err(e) = std::fs::write(&path, &content) {
                    app.error_message = Some(format!("Export failed: {}", e));
                } else {
                    app.status_message = format!("Exported to {}", path);
                }
            }
            Err(e) => {
                app.error_message = Some(format!("Export failed: {}", e));
            }
        }
    } else {
        app.error_message = Some("No repo open".to_string());
    }
}
