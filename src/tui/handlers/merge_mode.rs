use crossterm::event::{KeyCode, KeyEvent};
use lograph::operator::MergeMode;
use crate::tui::app::{App, PendingOp};
pub fn handle_merge_mode_popup(app: &mut App, key: KeyEvent) {
    const MODES: &[&str] = &["OR (Union)", "AND (Intersection)", "SUB (Subtract A-B)", "XOR (Symmetric diff)"];
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.show_merge_mode_popup = false;
            app.merge_sources.clear();
            app.history_marks.clear();
            app.status_message = String::from("Merge cancelled");
        }
        KeyCode::Enter => {
            let selected_mode = match app.merge_mode_cursor {
                0 => MergeMode::Union,
                1 => MergeMode::Intersection,
                2 => MergeMode::Subtract,
                _ => MergeMode::Xor,
            };
            let sources = std::mem::take(&mut app.merge_sources);
            let ids_str: Vec<String> = sources.iter().map(|i| i.to_string()).collect();
            let branch = format!("merge-{}", ids_str.join("-"));
            app.pending_op = PendingOp::MergeNodes {
                sources,
                branch,
                mode: selected_mode,
            };
            app.show_merge_mode_popup = false;
            app.history_marks.clear();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if app.merge_mode_cursor + 1 < MODES.len() {
                app.merge_mode_cursor += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.merge_mode_cursor > 0 {
                app.merge_mode_cursor -= 1;
            }
        }
        _ => {}
    }
}
