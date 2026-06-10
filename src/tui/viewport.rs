//! Viewport management — loading visible log lines, scrolling, and line count refresh.

use lograph::repo::LogRepo;

use crate::tui::app::App;

/// Load the viewport (visible slice of lines) from the current repo/log state.
pub fn load_viewport(app: &mut App) {
    if app.viewed_node_id.is_some() {
        let node_id = app.viewed_node_id.unwrap();
        let lines_result = app.get_node_lines(node_id).ok();
        if let Some(lines) = lines_result {
            app.total_lines = lines.len();
            app.line_count_is_original = node_id == 0;
            clamp_scroll_state(app);
            let start = app.scroll_offset.min(lines.len().saturating_sub(1));
            let end = (start + 200).min(lines.len());
            app.viewport_lines = lines[start..end].to_vec();
        } else {
            app.viewport_lines.clear();
        }
        return;
    }

    let repo_ref = app.repo.borrow();
    if repo_ref.is_none() {
        app.viewport_lines.clear();
        return;
    }

    let has_ops = repo_ref
        .as_ref()
        .map_or(false, |r| !r.history_tree().is_empty());
    drop(repo_ref);

    if has_ops {
        let total = {
            let mut repo_mut = app.repo.borrow_mut();
            repo_mut
                .as_mut()
                .map(|r| r.current_line_count().unwrap_or(0))
                .unwrap_or(0)
        };
        app.total_lines = total;
        clamp_scroll_state(app);

        let mut repo_mut = app.repo.borrow_mut();
        if let Some(ref mut r) = *repo_mut {
            app.viewport_lines = r
                .read_current_lines(app.scroll_offset, 200)
                .unwrap_or_default();
        }
    } else {
        app.total_lines = {
            let repo_ref = app.repo.borrow();
            repo_ref
                .as_ref()
                .map(|r: &LogRepo| r.original_line_count())
                .unwrap_or(0)
        };
        clamp_scroll_state(app);

        let repo_ref = app.repo.borrow();
        if let Some(ref r) = *repo_ref {
            app.viewport_lines = r
                .read_original_lines(app.scroll_offset, 200)
                .unwrap_or_default();
        }
    }
    app.line_count_is_original = !has_ops;
}

/// Clamp scroll_offset and cursor_line to the current total_lines range.
pub(crate) fn clamp_scroll_state(app: &mut App) {
    if app.total_lines > 0 {
        let max_offset = app.total_lines.saturating_sub(1);
        app.scroll_offset = app.scroll_offset.min(max_offset);
        app.cursor_line = app.cursor_line.min(max_offset);
    } else {
        app.scroll_offset = 0;
        app.cursor_line = 0;
    }
}

/// Refresh total_lines from the repo without loading the viewport.
pub fn refresh_line_count(app: &mut App) {
    let repo_ref = app.repo.borrow();
    if let Some(ref r) = *repo_ref {
        if r.history_tree().is_empty() {
            app.total_lines = r.original_line_count();
            app.line_count_is_original = true;
        }
    }
    drop(repo_ref);

    if !app.line_count_is_original {
        let mut repo_mut = app.repo.borrow_mut();
        if let Some(ref mut r) = *repo_mut {
            app.total_lines = r.current_line_count().unwrap_or(0);
        }
    }
}

// ── Scroll methods ──

pub fn scroll_down(app: &mut App, n: usize) {
    if app.total_lines == 0 {
        return;
    }
    let max_scroll = app.total_lines.saturating_sub(1);
    app.scroll_offset = (app.scroll_offset + n).min(max_scroll);
    load_viewport(app);
}

pub fn scroll_up(app: &mut App, n: usize) {
    app.scroll_offset = app.scroll_offset.saturating_sub(n);
    load_viewport(app);
}

pub fn go_to_line(app: &mut App, line: usize) {
    let max_line = app.total_lines.saturating_sub(1);
    app.cursor_line = line.min(max_line);
    if app.cursor_line < app.scroll_offset || app.cursor_line >= app.scroll_offset + 50 {
        app.scroll_offset = app.cursor_line.saturating_sub(15);
    }
    load_viewport(app);
}

pub fn scroll_right(app: &mut App, n: usize) {
    app.horizontal_scroll = app.horizontal_scroll.saturating_add(n);
}

pub fn scroll_left(app: &mut App, n: usize) {
    app.horizontal_scroll = app.horizontal_scroll.saturating_sub(n);
}

/// Go to line start (horizontal 0), like vim `0`.
pub fn go_to_line_start(app: &mut App) {
    app.horizontal_scroll = 0;
}

/// Go to line end (max horizontal scroll), like vim `$`.
pub fn go_to_line_end(app: &mut App) {
    let max_len = app
        .viewport_lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let content_width = app.terminal_width.saturating_sub(8) as usize;
    let visible_chars = content_width.max(20);
    app.horizontal_scroll = max_len.saturating_sub(visible_chars);
}

pub fn page_down(app: &mut App) {
    scroll_down(app, 40);
}

pub fn page_up(app: &mut App) {
    scroll_up(app, 40);
}
