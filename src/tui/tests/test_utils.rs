//! Shared test utilities for the TUI module.
//!
//! Provides helper functions to create test log data and initialize
//! an `App` with a temporary workspace and imported test log repo.

#![allow(dead_code)]

use std::fs;

use lograph::repo::Workspace;
use tempfile::TempDir;

use crate::tui::app::App;

/// Generate a synthetic log string with `lines` lines.
/// Lines alternate between INFO, WARN, ERROR, DEBUG levels.
pub fn make_test_log(lines: usize) -> String {
    (0..lines)
        .map(|i| {
            let level = match i % 4 {
                0 => "INFO",
                1 => "WARN",
                2 => "ERROR",
                _ => "DEBUG",
            };
            format!("2024-01-01 00:00:{:02} [{}] message {}", i % 60, level, i)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Create a temporary workspace, import a 200-line test log named "test",
/// and return an initialized `App`.
pub fn setup_app(tmp: &TempDir) -> App {
    let log_file = tmp.path().join("test.log");
    let content = make_test_log(200);
    fs::write(&log_file, &content).unwrap();
    let ws_root = tmp.path().join("workspace");
    let ws = Workspace::open(&ws_root).unwrap();
    let _ = ws.migrate_if_needed();
    ws.import_file("test", &log_file).unwrap();
    App::new(&ws_root, Some("test")).unwrap()
}

/// Create a temporary workspace, import a log from raw bytes, and return
/// an initialized `App`.
pub fn setup_app_custom(tmp: &TempDir, log_data: &[u8]) -> App {
    let log_file = tmp.path().join("test.log");
    fs::write(&log_file, log_data).unwrap();
    let ws_root = tmp.path().join("workspace");
    let ws = Workspace::open(&ws_root).unwrap();
    let _ = ws.migrate_if_needed();
    ws.import_file("test", &log_file).unwrap();
    App::new(&ws_root, Some("test")).unwrap()
}
