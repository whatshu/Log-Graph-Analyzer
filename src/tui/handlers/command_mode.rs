use crossterm::event::{KeyCode, KeyEvent};
use crate::tui::app::{App, InputMode};
use crate::tui::handlers::commands::execute_command;

pub fn command_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let cmd = app.input_buffer.clone();
            app.input_buffer.clear();
            app.input_mode = InputMode::Normal;
            execute_command(app, &cmd);
        }
        KeyCode::Char(c) => app.input_buffer.push(c),
        KeyCode::Backspace => { app.input_buffer.pop(); }
        _ => {}
    }
}
