use crossterm::event::{KeyCode, KeyEvent};
use crate::tui::app::{App, InputMode};
pub fn file_browser_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.status_message = String::from("File browser cancelled");
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.file_browser.move_down();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.file_browser.move_up();
        }
        KeyCode::Char('h') | KeyCode::Left => {
            // Go to parent directory
            if let Some(parent) = app.file_browser.current_dir.parent() {
                app.file_browser.current_dir = parent.to_path_buf();
                app.file_browser.selected_index = 0;
                app.file_browser.scroll_offset = 0;
                app.file_browser.refresh();
            }
        }
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
            if app.file_browser.enter_dir() {
                // File selected — import it
                app.import_from_file_browser();
            }
        }
        KeyCode::Char('.') => {
            app.file_browser.toggle_hidden();
        }
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Search;
            app.input_buffer.clear();
            app.input_prompt = String::from("Filter: ");
            // After search, apply filter
        }
        KeyCode::Char('g') => {
            app.file_browser.selected_index = 0;
            app.file_browser.scroll_offset = 0;
        }
        KeyCode::Char('G') => {
            let last = app.file_browser.entries.len().saturating_sub(1);
            app.file_browser.selected_index = last;
        }
        _ => {}
    }
}
