mod app;
mod event;
mod ui;
pub mod widgets;
pub mod handlers;
pub mod file_browser;

use std::io;
use std::path::Path;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::CrosstermBackend;

use app::{App, InputMode};
use event::{install_panic_hook, restore_terminal, setup_terminal};
use ui::render;

use lograph::error::Result;

pub fn run(workspace_root: &Path, initial_repo: Option<&str>) -> Result<()> {
    // Validate terminal capabilities before entering TUI mode.
    // TERM=dumb means we're in a non-interactive terminal (CI, pipes, etc.).
    match std::env::var("TERM") {
        Ok(ref t) if t == "dumb" || t.is_empty() => {
            eprintln!(
                "Error: TERM={} is not sufficient for the TUI. \
                 Please run in a terminal that supports ANSI escape sequences \
                 (xterm, gnome-terminal, tmux, etc.).",
                t
            );
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!(
                "Warning: TERM is not set. The TUI may not work correctly. \
                 Consider setting TERM=xterm-256color."
            );
        }
        _ => {} // OK — proceed
    }

    // Install a panic hook that restores the terminal so the user isn't
    // left with a broken terminal (raw mode, no cursor, stuck in alt screen)
    // if the app panics.
    install_panic_hook();

    // Clean terminal state on entry
    let _ = crossterm::execute!(
        io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    );

    let mut app = App::new(workspace_root, initial_repo)?;

    // Tmux: set window title if in tmux
    if app.in_tmux {
        tmux_set_title("lograph");
    }

    setup_terminal()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;

    let tick_rate = Duration::from_millis(100);
    let result = run_loop(&mut terminal, &mut app, tick_rate);

    restore_terminal()?;

    // Tmux: reset title
    if app.in_tmux {
        tmux_set_title("lograph (exited)");
    }

    result.map_err(|e| lograph::error::LogAnalyzerError::Io(e))?;
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
    // Ctrl+C always quits
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    // If help overlay is showing, close it on q, Esc, or ?
    if app.show_help {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') => {
                app.show_help = false;
                return;
            }
            _ => {}
        }
    }

    // If collect detail popup is showing, close it on c, q, or Esc
    if app.show_collect_detail {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('q') | KeyCode::Esc => {
                app.show_collect_detail = false;
                return;
            }
            _ => {}
        }
    }

    // Esc behavior: in file browser, cancel; otherwise clear input modes
    if key.code == KeyCode::Esc {
        match app.input_mode {
            InputMode::FileBrowser => {
                app.input_mode = InputMode::Normal;
                app.status_message = String::from("cancelled");
                return;
            }
            InputMode::Normal => {
                app.error_message = None;
                app.status_message.clear();
                return;
            }
            _ => {
                app.input_mode = InputMode::Normal;
                app.input_buffer.clear();
                app.status_message = String::from("cancelled");
                return;
            }
        }
    }

    // Route to the correct handler based on input mode
    match app.input_mode {
        InputMode::Normal => handlers::normal_mode(app, key),
        InputMode::Command => handlers::command_mode(app, key),
        InputMode::Search => handlers::search_mode(app, key),
        InputMode::Input => handlers::input_mode(app, key),
        InputMode::FileBrowser => handlers::file_browser_mode(app, key),
        InputMode::VisualSelect => handlers::visual_select_mode(app, key),
        InputMode::TagRename => handlers::tag_rename_mode(app, key),
    }
}

/// Set tmux window title via ANSI escape sequences.
fn tmux_set_title(title: &str) {
    // Set both window title and icon name
    print!("\x1b]0;{}\x07", title);
    // Also set xterm window title
    print!("\x1b]2;{}\x07", title);
}
