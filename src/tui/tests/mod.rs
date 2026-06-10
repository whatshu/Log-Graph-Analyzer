//! TUI unit tests — organized by the module they cover.
//!
//! Test files mirror the source module structure:
//! - [`app`] — tests for [`super::app`] (application state and business logic)
//! - [`handlers`] — tests for [`super::handlers`] (input handling and command parsing)
//!
//! Shared helpers live in [`test_utils`].

mod app;
mod handlers;
mod test_utils;
