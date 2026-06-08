use std::io;

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
