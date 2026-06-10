//! Input handling modules.
//!
//! Each submodule handles a specific input mode or popup.
//! The top-level key dispatcher in [`super::mod`] routes to these handlers.

pub(crate) mod normal_mode;
mod command_mode;
mod search_mode;
mod input_mode;
mod file_browser_mode;
pub mod commands;
mod tag_manager;
mod merge_mode;

// Re-export public handler functions for use by the key dispatcher.
pub use normal_mode::normal_mode;
pub use command_mode::command_mode;
pub use search_mode::search_mode;
pub use input_mode::input_mode;
pub use file_browser_mode::file_browser_mode;
pub use tag_manager::handle_tag_manager_popup;
pub use merge_mode::handle_merge_mode_popup;
