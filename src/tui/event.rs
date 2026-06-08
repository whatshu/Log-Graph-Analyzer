use std::io;
use std::panic;

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::cursor;

pub fn setup_terminal() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, cursor::Hide)
}

pub fn restore_terminal() -> io::Result<()> {
    execute!(io::stdout(), LeaveAlternateScreen, cursor::Show)?;
    disable_raw_mode()
}

/// Install a panic hook that restores the terminal before printing the panic
/// message. This prevents users from being left with a broken terminal (raw
/// mode + missing cursor + stuck in alternate screen) if the TUI panics.
pub fn install_panic_hook() {
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        // Best-effort restore — ignore errors since we're already panicking.
        let _ = restore_terminal();
        prev_hook(info);
    }));
}
