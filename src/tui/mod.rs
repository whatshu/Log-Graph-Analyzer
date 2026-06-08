mod app;
mod event;
mod ui;
pub mod widgets;
pub mod handlers;

use std::io;
use std::path::Path;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::CrosstermBackend;

use app::{App, InputMode};
use event::{restore_terminal, setup_terminal};
use ui::render;

use log_analyzer_core::error::Result;

pub fn run(workspace_root: &Path, initial_repo: Option<&str>) -> Result<()> {
    // Ensure terminal is clean on entry
    let _ = crossterm::execute!(
        io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    );

    let mut app = App::new(workspace_root, initial_repo)?;

    setup_terminal()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;

    let tick_rate = Duration::from_millis(100);
    let result = run_loop(&mut terminal, &mut app, tick_rate);

    restore_terminal()?;
    result.map_err(|e| log_analyzer_core::error::LogAnalyzerError::Io(e))?;

    Ok(())
}

fn run_loop(
    terminal: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tick_rate: Duration,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| render(f, app))?;

        if app.should_quit {
            break;
        }

        if crossterm::event::poll(tick_rate)? {
            if let Event::Key(key) = crossterm::event::read()? {
                handle_key(app, key);
            }
        }

        app.apply_pending();
    }

    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    if key.code == KeyCode::Esc {
        match app.input_mode {
            InputMode::Normal => {
                app.error_message = None;
                app.status_message.clear();
            }
            _ => {
                app.input_mode = InputMode::Normal;
                app.input_buffer.clear();
                app.status_message = String::from("cancelled");
            }
        }
        return;
    }

    match app.input_mode {
        InputMode::Normal => handlers::normal_mode(app, key),
        InputMode::Command => handlers::command_mode(app, key),
        InputMode::Search => handlers::search_mode(app, key),
        InputMode::Input => handlers::input_mode(app, key),
    }
}
