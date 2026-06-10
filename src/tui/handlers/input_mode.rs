use crossterm::event::{KeyCode, KeyEvent};
use crate::tui::app::{App, InputMode};
use crate::tui::handlers::commands::handle_input;
pub fn input_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let input = app.input_buffer.clone();
            let prompt = app.input_prompt.clone();
            app.input_buffer.clear();
            app.input_mode = InputMode::Normal;
            handle_input(app, &prompt, &input);
        }
        KeyCode::Char(c) => app.input_buffer.push(c),
        KeyCode::Backspace => { app.input_buffer.pop(); }
        _ => {}
    }
}

