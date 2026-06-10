//! Collect operations — run collectors and format their results.

use lograph::engine::{CollectResult, Collector};
use lograph::operator::Operation;

use crate::tui::app::App;

/// Run a collector against the current repo state and create a history node.
/// The collect result is formatted as text lines and stored as the node's
/// log content, and the full detail is shown as a popup.
pub fn run_collect(app: &mut App, collector: Collector) {
    let result = {
        let mut repo_mut = app.repo.borrow_mut();
        repo_mut
            .as_mut()
            .and_then(|r| r.collect(&collector).ok())
    };

    match result {
        Some(ref r) => {
            let summary = r.summary();
            let detail = r.to_detail_string();
            app.collect_detail = Some(detail);
            app.show_collect_detail = true;
            app.status_message = format!("Collect: {}", summary);
            app.pending_collect_summary = Some(summary);
            app.queue_operation(Operation::Collect { collector });
        }
        None => {
            app.error_message = Some("Collect failed".to_string());
        }
    }
}

/// Format a one-line summary of a collect result.
pub fn collect_result_summary(result: &CollectResult) -> String {
    result.summary()
}

/// Format a multi-line detail display of a collect result.
pub fn collect_result_detail(result: &CollectResult) -> String {
    result.to_detail_string()
}
