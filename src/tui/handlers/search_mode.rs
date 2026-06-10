use crossterm::event::{KeyCode, KeyEvent};
use crate::tui::app::{App, InputMode};
pub fn search_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let query = app.input_buffer.clone();
            app.input_buffer.clear();
            app.input_mode = InputMode::Normal;
            app.history_reset();
            if !query.is_empty() {
                app.add_to_history(&query);
                app.do_search(&query);
            }
        }
        KeyCode::Up => {
            // Navigate search history (up = older)
            if let Some(term) = app.history_navigate_up() {
                app.input_buffer = term.to_string();
            }
        }
        KeyCode::Down => {
            // Navigate search history (down = newer)
            if let Some(term) = app.history_navigate_down() {
                app.input_buffer = term.to_string();
            }
        }
        KeyCode::Char(c) => {
            app.history_reset();
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => {
            app.history_reset();
            app.input_buffer.pop();
        }
        _ => {}
    }
}
