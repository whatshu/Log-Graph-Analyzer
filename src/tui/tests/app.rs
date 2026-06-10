//! Tests for [`super::app`] — application state and business logic.

use std::fs;

use lograph::engine::{CollectResult, Collector};
use lograph::operator::{MergeMode, Operation};
use tempfile::TempDir;

use crate::tui::app::{App, InputMode, PendingOp, ViewKind};
use crate::tui::tests::test_utils::{setup_app, setup_app_custom};

// ── App creation ──

#[test]
fn test_app_new_with_repo() {
    let tmp = TempDir::new().unwrap();
    let app = setup_app(&tmp);

    assert_eq!(app.repo_name, "test");
    assert_eq!(app.total_lines, 200);
    assert!(!app.viewport_lines.is_empty());
    assert_eq!(app.active_view, ViewKind::LogView);
    assert_eq!(app.input_mode, InputMode::Normal);
    assert!(!app.should_quit);
}

#[test]
fn test_app_new_empty_workspace() {
    let tmp = TempDir::new().unwrap();
    let ws_root = tmp.path().join("empty_ws");
    fs::create_dir_all(&ws_root).unwrap();
    let app = App::new(&ws_root, None).unwrap();

    assert!(app.repo_name.is_empty());
    assert_eq!(app.total_lines, 0);
    assert!(app.viewport_lines.is_empty());
}

// ── Search ──

#[test]
fn test_do_search_finds_matches() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.do_search("ERROR");
    assert!(!app.search_results.is_empty());
    assert_eq!(app.search_query, "ERROR");
    // Should jump to first match
    assert!(app.cursor_line < 200);
}

#[test]
fn test_do_search_no_match() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.do_search("NO_SUCH_TEXT_ANYWHERE");
    assert!(app.search_results.is_empty());
    assert_eq!(app.status_message, "No matches found");
}

#[test]
fn test_next_and_prev_match() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.do_search("message 5");
    assert!(!app.search_results.is_empty());
    let first = app.cursor_line;

    app.next_match();
    assert_ne!(app.cursor_line, first);
    let _second = app.cursor_line;

    app.prev_match();
    assert_eq!(app.cursor_line, first);

    app.prev_match(); // wraps around to last match
    assert_ne!(app.cursor_line, first);
    app.next_match(); // wraps around to first again
    assert_eq!(app.cursor_line, first);
}

// ── Filter ──

#[test]
fn test_filter_keep_reduces_lines() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    assert_eq!(app.total_lines, 200);
    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();

    // ERROR is every 4th line → 50 lines
    assert_eq!(app.total_lines, 50);
    assert!(app.viewport_lines.iter().all(|l| l.contains("ERROR")));
}

#[test]
fn test_filter_remove() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: false,
    });
    app.apply_pending();

    // 200 - 50 = 150 lines
    assert_eq!(app.total_lines, 150);
    assert!(!app.viewport_lines.iter().any(|l| l.contains("ERROR")));
}

// ── Undo ──

#[test]
fn test_undo_restores_state() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();
    assert_eq!(app.total_lines, 50);
    assert!(!app.line_count_is_original);

    app.queue_undo();
    app.apply_pending();
    assert_eq!(app.total_lines, 200);
    // Note: line_count_is_original may stay false with non-destructive undo
    // since history tree nodes are preserved
}

// ── Scroll offset clamping (regression test for the blank-viewport bug) ──

#[test]
fn test_scroll_offset_clamped_after_filter() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Simulate a search that moves scroll far down
    app.scroll_offset = 150;
    assert!(app.scroll_offset > 0);

    // Apply a filter that drastically reduces the dataset
    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();

    // After load_viewport, scroll_offset should be clamped to < total_lines
    assert!(app.scroll_offset < app.total_lines);
    assert!(!app.viewport_lines.is_empty());
    assert!(app.cursor_line < app.total_lines);
}

#[test]
fn test_scroll_offset_clamped_to_zero_when_empty() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.scroll_offset = 50;

    // Filter to zero lines (pattern never matches)
    app.queue_operation(Operation::Filter {
        pattern: "ZZZ_NONEXISTENT_ZZZ".to_string(),
        keep: true,
    });
    app.apply_pending();

    assert_eq!(app.total_lines, 0);
    assert_eq!(app.scroll_offset, 0);
    assert_eq!(app.cursor_line, 0);
}

// ── Horizontal scroll ──

#[test]
fn test_horizontal_scroll_left_right() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    assert_eq!(app.horizontal_scroll, 0);
    app.scroll_right(20);
    assert_eq!(app.horizontal_scroll, 20);
    app.scroll_left(8);
    assert_eq!(app.horizontal_scroll, 12);
    app.scroll_left(50);
    assert_eq!(app.horizontal_scroll, 0); // saturating_sub
}

#[test]
fn test_go_to_line_start() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.scroll_right(50);
    assert!(app.horizontal_scroll > 0);
    app.go_to_line_start();
    assert_eq!(app.horizontal_scroll, 0);
}

#[test]
fn test_go_to_line_end() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Ensure we have lines with some length
    assert!(!app.viewport_lines.is_empty());
    app.go_to_line_end();
    // Either 0 (all lines fit) or > 0 (some lines wider)
    // We can at least assert it doesn't panic
}

// ── Input modes ──

#[test]
fn test_input_mode_transitions() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    assert_eq!(app.input_mode, InputMode::Normal);

    // Enter search mode
    app.input_mode = InputMode::Search;
    app.input_buffer = "test_query".to_string();
    assert_eq!(app.input_mode, InputMode::Search);
    assert_eq!(app.input_buffer, "test_query");

    // Enter command mode
    app.input_mode = InputMode::Command;
    app.input_buffer = ":f ERROR".to_string();
    assert_eq!(app.input_mode, InputMode::Command);
}

// ── Search history ──

#[test]
fn test_add_to_history_dedup() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Clear any pre-loaded history for predictable tests
    app.search_history.clear();

    app.add_to_history("error");
    assert_eq!(app.search_history[0], "error");
    assert_eq!(app.search_history.len(), 1);

    app.add_to_history("warn");
    assert_eq!(app.search_history[0], "warn");
    assert_eq!(app.search_history[1], "error");
    assert_eq!(app.search_history.len(), 2);

    // Adding again moves to front without increasing length
    app.add_to_history("error");
    assert_eq!(app.search_history[0], "error");
    assert_eq!(app.search_history[1], "warn");
    assert_eq!(app.search_history.len(), 2);
}

#[test]
fn test_history_navigation() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Clear pre-loaded history for predictable navigation
    app.search_history.clear();
    app.add_to_history("first");
    app.add_to_history("second");
    app.search_history_idx = -1;

    // History is most-recent-first: ["second", "first"]
    // Navigate up (older) → "second" first
    let result = app.history_navigate_up();
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "second");

    // Navigate up again → "first"
    let result = app.history_navigate_up();
    assert_eq!(result.unwrap(), "first");

    // Navigate down → back to "second"
    let result = app.history_navigate_down();
    assert_eq!(result.unwrap(), "second");

    // Navigate down again → empty (past the newest)
    let result = app.history_navigate_down();
    assert_eq!(result.unwrap(), "");
}

// ── View switching ──

#[test]
fn test_view_switching() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    assert_eq!(app.active_view, ViewKind::LogView);

    app.active_view = ViewKind::History;
    assert_eq!(app.active_view, ViewKind::History);

    app.active_view = ViewKind::RepoList;
    assert_eq!(app.active_view, ViewKind::RepoList);

    app.active_view = ViewKind::Analytics;
    assert_eq!(app.active_view, ViewKind::Analytics);
}

// ── Help toggle ──

#[test]
fn test_help_toggle() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    assert!(!app.show_help);
    app.show_help = true;
    assert!(app.show_help);
    app.show_help = false;
    assert!(!app.show_help);
}

// ── Go to line ──

#[test]
fn test_go_to_line() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.go_to_line(50);
    assert_eq!(app.cursor_line, 50);
    // scroll_offset should be near the cursor
    assert!(app.scroll_offset <= 50);
}

#[test]
fn test_go_to_line_clamped() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.go_to_line(9999); // beyond end
    assert_eq!(app.cursor_line, 199); // clamped to last line
}

// ── Page up/down ──

#[test]
fn test_page_down_and_up() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    let initial = app.scroll_offset;
    app.page_down();
    assert!(app.scroll_offset > initial);
    app.page_up();
    assert_eq!(app.scroll_offset, initial);
}

// ── Collect operations ──

#[test]
fn test_collect_result_summary_count() {
    assert_eq!(
        App::collect_result_summary(&CollectResult::Count(42)),
        "Count: 42"
    );
    assert_eq!(
        App::collect_result_summary(&CollectResult::Count(0)),
        "Count: 0"
    );
}

#[test]
fn test_collect_result_summary_group_count() {
    let pairs = vec![
        ("ERROR".to_string(), 100),
        ("WARN".to_string(), 50),
    ];
    let summary = App::collect_result_summary(&CollectResult::GroupCount(pairs));
    assert!(summary.contains("GroupCount: 2 groups"));
    assert!(summary.contains("ERROR"));
}

#[test]
fn test_collect_result_summary_topn() {
    let pairs = vec![("1.1.1.1".to_string(), 300)];
    let summary = App::collect_result_summary(&CollectResult::TopN(pairs));
    assert!(summary.contains("Top1"));
    assert!(summary.contains("1.1.1.1"));
}

#[test]
fn test_collect_result_summary_unique() {
    let vals = vec!["alice".to_string(), "bob".to_string()];
    let summary = App::collect_result_summary(&CollectResult::Unique(vals));
    assert_eq!(summary, "Unique: 2 values");
}

#[test]
fn test_collect_result_summary_numeric_stats() {
    let summary = App::collect_result_summary(&CollectResult::NumericStats {
        count: 10,
        sum: 55.0,
        min: 1.0,
        max: 10.0,
        avg: 5.5,
    });
    assert!(summary.contains("NumStats"));
    assert!(summary.contains("n=10"));
}

#[test]
fn test_collect_result_summary_line_stats() {
    let summary = App::collect_result_summary(&CollectResult::LineStats {
        count: 100,
        total_bytes: 5000,
        avg_len: 50.0,
        max_len: 100,
        min_len: 10,
    });
    assert!(summary.contains("LineStats"));
    assert!(summary.contains("n=100"));
}

#[test]
fn test_collect_result_summary_empty_group_count() {
    let pairs: Vec<(String, usize)> = vec![];
    let summary = App::collect_result_summary(&CollectResult::GroupCount(pairs));
    assert!(summary.contains("GroupCount: 0 groups"));
    assert!(summary.contains("—")); // fallback for empty
}

#[test]
fn test_collect_result_detail_count() {
    let detail = App::collect_result_detail(&CollectResult::Count(99));
    assert_eq!(detail, "Count: 99 lines");
}

#[test]
fn test_collect_result_detail_group_count() {
    let pairs = vec![("error".to_string(), 5), ("warn".to_string(), 2)];
    let detail = App::collect_result_detail(&CollectResult::GroupCount(pairs));
    assert!(detail.contains("Group Count"));
    assert!(detail.contains("error"));
    assert!(detail.contains("5"));
    assert!(detail.contains("warn"));
    assert!(detail.contains("2"));
}

#[test]
fn test_collect_result_detail_topn() {
    let pairs = vec![("x".to_string(), 1)];
    let detail = App::collect_result_detail(&CollectResult::TopN(pairs));
    assert!(detail.contains("Top-1"));
    assert!(detail.contains("x"));
}

#[test]
fn test_collect_result_detail_unique() {
    let vals = vec!["a".to_string(), "b".to_string()];
    let detail = App::collect_result_detail(&CollectResult::Unique(vals));
    assert!(detail.contains("2 distinct"));
    assert!(detail.contains("a"));
    assert!(detail.contains("b"));
}

#[test]
fn test_collect_result_detail_numeric_stats() {
    let detail = App::collect_result_detail(&CollectResult::NumericStats {
        count: 5,
        sum: 15.0,
        min: 1.0,
        max: 5.0,
        avg: 3.0,
    });
    assert!(detail.contains("Numeric Statistics"));
    assert!(detail.contains("Count:  5"));
    assert!(detail.contains("Min:    1.0000"));
    assert!(detail.contains("Max:    5.0000"));
    assert!(detail.contains("Avg:    3.0000"));
}

#[test]
fn test_collect_result_detail_line_stats() {
    let detail = App::collect_result_detail(&CollectResult::LineStats {
        count: 10,
        total_bytes: 500,
        avg_len: 50.0,
        max_len: 100,
        min_len: 10,
    });
    assert!(detail.contains("Line Statistics"));
    assert!(detail.contains("Lines:       10"));
    assert!(detail.contains("Max Length:  100"));
    assert!(detail.contains("Min Length:  10"));
}

#[test]
fn test_run_collect_count_all() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.run_collect(Collector::Count { pattern: None });
    assert!(app.show_collect_detail);
    assert!(app.collect_detail.is_some());
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Count: 200 lines"));

    // Collect is queued as an operation. Apply it to create the node.
    app.apply_pending();

    // After apply_pending, the collect summary should be stored at the new HEAD.
    let head = app.repo.borrow().as_ref().unwrap().head_node_id();
    assert!(app.collect_results.contains_key(&head));
    assert_eq!(app.collect_results.get(&head).unwrap(), "Count: 200");
}

#[test]
fn test_run_collect_count_with_pattern() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.run_collect(Collector::Count {
        pattern: Some("ERROR".to_string()),
    });
    assert!(app.show_collect_detail);
    let detail = app.collect_detail.as_ref().unwrap();
    // ERROR appears every 4th line in 200 lines → 50
    assert!(detail.contains("Count: 50 lines"));
}

#[test]
fn test_run_collect_linestats() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.run_collect(Collector::LineStats);
    assert!(app.show_collect_detail);
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Line Statistics"));
    assert!(detail.contains("Lines:       200"));
    // Each line has "2024-01-01 00:00:XX [LEVEL] message N" ~45 chars
    assert!(detail.contains("Max Length:"));
    assert!(detail.contains("Min Length:"));
}

#[test]
fn test_run_collect_group_count() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.run_collect(Collector::GroupCount {
        pattern: r"\[(\w+)\]".to_string(),
        group_index: 1,
    });
    assert!(app.show_collect_detail);
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Group Count"));
    // Should find INFO, WARN, ERROR, DEBUG
    assert!(detail.contains("INFO"));
    assert!(detail.contains("ERROR"));
    assert!(detail.contains("WARN"));
    assert!(detail.contains("DEBUG"));
}

#[test]
fn test_run_collect_unique() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.run_collect(Collector::Unique {
        pattern: r"\[(\w+)\]".to_string(),
        group_index: 1,
    });
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("4 distinct")); // INFO, WARN, ERROR, DEBUG
}

#[test]
fn test_run_collect_topn() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.run_collect(Collector::TopN {
        pattern: r"\[(\w+)\]".to_string(),
        group_index: 1,
        n: 2,
    });
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Top-2"));
}

#[test]
fn test_run_collect_numeric_stats() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // message number at end of each line is numeric
    app.run_collect(Collector::NumericStats {
        pattern: r"message (\d+)".to_string(),
        group_index: 1,
    });
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Numeric Statistics"));
    assert!(detail.contains("Count:  200"));
}

#[test]
fn test_collect_result_appears_in_history_node() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Run a collect and apply it to create a history node
    app.run_collect(Collector::Count { pattern: None });
    app.apply_pending();
    app.build_history();

    // The collect node (node 1) should have the collect summary
    let collect_node = app.history_nodes.iter().find(|n| n.id == 1);
    assert!(collect_node.is_some());
    let summary = collect_node.unwrap().collect_summary.as_ref();
    assert_eq!(summary.unwrap(), "Count: 200");
}

#[test]
fn test_collect_detail_popup_closes() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.run_collect(Collector::Count { pattern: None });
    assert!(app.show_collect_detail);

    app.show_collect_detail = false;
    assert!(!app.show_collect_detail);

    // Status message should have been set
    assert!(app.status_message.contains("Collect:"));
}

#[test]
fn test_run_collect_updates_status_message() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.run_collect(Collector::LineStats);
    assert!(app.status_message.starts_with("Collect:"));
    assert!(app.status_message.contains("LineStats"));
}

#[test]
fn test_run_collect_remembers_multiple_results() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Run collect at node 0 and apply it
    app.run_collect(Collector::Count { pattern: None });
    app.apply_pending();
    let first_head = app.repo.borrow().as_ref().unwrap().head_node_id();
    assert_eq!(app.collect_results.get(&first_head).unwrap(), "Count: 200");

    // Undo back to root to get original data back
    app.queue_undo();
    app.apply_pending();

    // Apply a filter to reduce lines, then collect again
    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();

    app.run_collect(Collector::Count { pattern: None });
    app.apply_pending();
    let second_head = app.repo.borrow().as_ref().unwrap().head_node_id();
    assert!(app.collect_results.contains_key(&second_head));
    assert_eq!(app.collect_results.get(&second_head).unwrap(), "Count: 50");
}

// ── clear_search ──

#[test]
fn test_clear_search_empties_state() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Set up search state
    app.search_query = "ERROR".to_string();
    app.search_results = vec![2, 6, 10];
    app.search_index = 1;

    app.clear_search();

    assert!(app.search_query.is_empty());
    assert!(app.search_results.is_empty());
    assert_eq!(app.search_index, 0);
}

#[test]
fn test_filter_keep_clears_search() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Simulate user: search then filter
    app.do_search("ERROR");
    assert!(!app.search_results.is_empty());
    assert_eq!(app.search_query, "ERROR");

    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();

    // Search should be cleared after filter
    assert!(app.search_query.is_empty());
    assert!(app.search_results.is_empty());
    assert_eq!(app.search_index, 0);
}

#[test]
fn test_filter_remove_clears_search() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.do_search("ERROR");
    assert!(!app.search_results.is_empty());

    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: false,
    });
    app.apply_pending();

    assert!(app.search_query.is_empty());
    assert!(app.search_results.is_empty());
}

#[test]
fn test_replace_clears_search() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.do_search("ERROR");
    assert!(!app.search_results.is_empty());

    app.queue_operation(Operation::Replace {
        pattern: "ERROR".to_string(),
        replacement: "OK".to_string(),
    });
    app.apply_pending();

    assert!(app.search_query.is_empty());
    assert!(app.search_results.is_empty());
}

#[test]
fn test_undo_clears_search() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Apply a filter first
    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();

    // Search in the filtered state
    app.do_search("message");
    assert!(!app.search_results.is_empty());

    // Undo should clear search
    app.queue_undo();
    app.apply_pending();

    assert!(app.search_query.is_empty());
    assert!(app.search_results.is_empty());
}

// ── Checkout uses node ID (not cursor index) ──

#[test]
fn test_queue_checkout_stores_node_id() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Create a history where cursor index != node ID is possible.
    // Apply operations to create multiple nodes.
    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();
    app.queue_operation(Operation::Filter {
        pattern: "message 0".to_string(),
        keep: true,
    });
    app.apply_pending();
    app.build_history();

    // history_nodes should have 3 entries: node 0 (import),
    // node 1 (filter ERROR), node 2 (filter message 0).
    // Cursor position of HEAD should be at index 2 (node_id=2).
    // But let's test with the first non-root node explicitly.
    let _node1 = app.history_nodes.iter().find(|n| n.id == 1).unwrap();
    let node1_index = app.history_nodes.iter().position(|n| n.id == 1).unwrap();

    // Directly queue checkout with the actual node ID (simulating
    // what the fixed Enter handler does).
    let actual_node_id = app.history_nodes[node1_index].id;
    app.queue_checkout(actual_node_id);

    match &app.pending_op {
        PendingOp::CheckoutTo(id) => assert_eq!(*id, actual_node_id),
        other => panic!("Expected CheckoutTo, got {:?}", other),
    }
}

#[test]
fn test_apply_operation_from_clears_search() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // First filter to create a non-root node
    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();

    // Search
    app.do_search("message");
    assert!(!app.search_results.is_empty());

    // View node 0, then apply operation from it (branch off)
    app.viewed_node_id = Some(0);
    app.detached_head = true;
    app.queue_operation(Operation::Filter {
        pattern: "DEBUG".to_string(),
        keep: true,
    });
    app.apply_pending();

    // Search should be cleared
    assert!(app.search_query.is_empty());
    assert!(app.search_results.is_empty());
}

// ── Merge + view content correctness ──

#[test]
fn test_merge_view_content_correctness() {
    let tmp = TempDir::new().unwrap();
    // Create log with distinct 201 and 200 lines
    let log_data = b"line with 201 code\nline with 200 code\nanother 201 line\nanother 200 line\n";
    let mut app = setup_app_custom(&tmp, log_data);

    // Step 1: Apply filter keep 201 → creates node 1
    app.queue_operation(Operation::Filter {
        pattern: "201".to_string(),
        keep: true,
    });
    app.apply_pending();

    // Step 2: Undo → back to root
    app.queue_undo();
    app.apply_pending();

    // Step 3: Apply filter keep 200 from root → creates node 2
    app.queue_operation(Operation::Filter {
        pattern: "200".to_string(),
        keep: true,
    });
    app.apply_pending();

    // Build history so we can see the nodes
    app.build_history();
    assert_eq!(app.history_nodes.len(), 3, "should have root + 2 filter nodes");

    // Verify node IDs are 1 and 2 (node 0 is root/import)
    let node1 = app.history_nodes.iter().find(|n| n.id == 1).expect("node 1 not found");
    let node2 = app.history_nodes.iter().find(|n| n.id == 2).expect("node 2 not found");
    assert_ne!(node1.id, node2.id);

    // Step 4: Verify source node content BEFORE merge
    let n1_before = app.get_node_lines(1).expect("get_node_lines(1) before merge");
    assert!(n1_before.iter().all(|l| l.contains("201")),
        "node 1 before merge should only have 201 lines, got: {:?}", n1_before);

    let n2_before = app.get_node_lines(2).expect("get_node_lines(2) before merge");
    assert!(n2_before.iter().all(|l| l.contains("200")),
        "node 2 before merge should only have 200 lines, got: {:?}", n2_before);

    assert_ne!(n1_before, n2_before, "nodes 1 and 2 should have different content before merge");

    // Step 5: Mark both nodes and execute merge
    app.history_marks.insert(1);
    app.history_marks.insert(2);
    let mut sources: Vec<usize> = app.history_marks.iter().copied().collect();
    sources.sort_unstable();
    app.pending_op = PendingOp::MergeNodes {
        sources,
        branch: "merge-1-2".to_string(),
        mode: MergeMode::Union,
    };
    app.apply_pending();

    // Step 6: Build history to see the merge node
    app.build_history();
    let merge_node = app.history_nodes.iter().find(|n| n.id == 3).expect("merge node 3 not found");
    assert!(merge_node.description.contains("merge"), "node 3 should be a merge node");

    // Step 7: CRITICAL — verify source nodes UNCHANGED after merge
    let n1_after = app.get_node_lines(1).expect("get_node_lines(1) after merge");
    assert!(n1_after.iter().all(|l| l.contains("201")),
        "BUG: node 1 after merge lost its 201 content, got: {:?}", n1_after);
    assert_eq!(n1_after, n1_before,
        "BUG: node 1 content changed after merge! before={:?} after={:?}", n1_before, n1_after);

    let n2_after = app.get_node_lines(2).expect("get_node_lines(2) after merge");
    assert!(n2_after.iter().all(|l| l.contains("200")),
        "BUG: node 2 after merge lost its 200 content, got: {:?}", n2_after);
    assert_eq!(n2_after, n2_before,
        "BUG: node 2 content changed after merge! before={:?} after={:?}", n2_before, n2_after);

    assert_ne!(n1_after, n2_after,
        "BUG: nodes 1 and 2 have identical content after merge");

    // Step 8: Verify merge node has UNION of both sources
    let n3 = app.get_node_lines(3).expect("get_node_lines(3) after merge");
    assert_eq!(n3.len(), 4, "merge should have 4 lines (2 from 201 + 2 from 200), got {}: {:?}", n3.len(), n3);
    let has_201 = n3.iter().any(|l| l.contains("201"));
    let has_200 = n3.iter().any(|l| l.contains("200"));
    assert!(has_201, "merge should contain 201 lines, got: {:?}", n3);
    assert!(has_200, "merge should contain 200 lines, got: {:?}", n3);
}

// ── Tag system tests ──

#[test]
fn test_create_tag_via_store() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    let tag = lograph::tag::Tag {
        name: "test-tag".into(),
        ranges: vec![(10, 20)],
        created_at: chrono::Utc::now(),
    };
    app.tag_store.add_tag(&app.repo_name, tag);
    let _ = app.tag_store.save(&app.workspace.root());

    let tags = app.tag_store.get_tags(&app.repo_name);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "test-tag");
    assert_eq!(tags[0].ranges, vec![(10, 20)]);
}

#[test]
fn test_tag_rename() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    let tag = lograph::tag::Tag {
        name: "old".into(),
        ranges: vec![(5, 10)],
        created_at: chrono::Utc::now(),
    };
    app.tag_store.add_tag(&app.repo_name, tag);
    app.tag_store.rename_tag(&app.repo_name, "old", "new");
    let _ = app.tag_store.save(&app.workspace.root());

    let tags = app.tag_store.get_tags(&app.repo_name);
    assert_eq!(tags[0].name, "new");
}

#[test]
fn test_tag_delete() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.tag_store.add_tag(&app.repo_name, lograph::tag::Tag {
        name: "del-me".into(),
        ranges: vec![(1, 3)],
        created_at: chrono::Utc::now(),
    });
    assert_eq!(app.tag_store.get_tags(&app.repo_name).len(), 1);

    app.tag_store.remove_tag(&app.repo_name, "del-me");
    assert_eq!(app.tag_store.get_tags(&app.repo_name).len(), 0);
}

#[test]
fn test_tag_remap_after_filter_keep() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Create a tag at lines 0-10
    app.tag_store.add_tag(&app.repo_name, lograph::tag::Tag {
        name: "top".into(),
        ranges: vec![(0, 10)],
        created_at: chrono::Utc::now(),
    });

    // Filter keep ERROR (every 4th line: 2, 6, 10 → 3 lines in 0-10)
    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();

    // Tag should survive with remapped ranges
    let tags = app.tag_store.get_tags(&app.repo_name);
    assert!(!tags.is_empty(), "Tag should survive filter keep");
    let tag = &tags[0];
    assert_eq!(tag.name, "top");
    // Only lines 2, 6, 10 matching ERROR within old [0,10] survive
    // After filter, they become lines 0, 1, 2 (since they're the first 3 ERROR lines)
    assert!(!tag.ranges.is_empty(), "Tag should have ranges after remap");
}

#[test]
fn test_tag_remap_after_filter_remove() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Create a tag at lines 0-10
    app.tag_store.add_tag(&app.repo_name, lograph::tag::Tag {
        name: "top".into(),
        ranges: vec![(0, 10)],
        created_at: chrono::Utc::now(),
    });

    // Filter remove ERROR
    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: false,
    });
    app.apply_pending();

    // Tag should survive with remapped ranges
    let tags = app.tag_store.get_tags(&app.repo_name);
    assert!(!tags.is_empty(), "Tag should survive filter remove");
}

#[test]
fn test_tags_cleared_after_collect() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Create a tag
    app.tag_store.add_tag(&app.repo_name, lograph::tag::Tag {
        name: "will-die".into(),
        ranges: vec![(0, 50)],
        created_at: chrono::Utc::now(),
    });
    assert!(!app.tag_store.get_tags(&app.repo_name).is_empty());

    // Run collect
    app.run_collect(Collector::Count { pattern: None });
    app.apply_pending();

    // Tags should be cleared
    assert!(app.tag_store.get_tags(&app.repo_name).is_empty());
}

#[test]
fn test_tag_remap_after_delete_lines() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Create tag at lines 10-20
    app.tag_store.add_tag(&app.repo_name, lograph::tag::Tag {
        name: "middle".into(),
        ranges: vec![(10, 20)],
        created_at: chrono::Utc::now(),
    });

    // Delete lines 0-5 (6 lines)
    app.queue_operation(Operation::DeleteLines {
        line_indices: (0..=5).collect(),
    });
    app.apply_pending();

    // Tag range should shift up by 6
    let tags = app.tag_store.get_tags(&app.repo_name);
    assert!(!tags.is_empty());
    let tag = &tags[0];
    assert_eq!(tag.ranges[0], (4, 14), "Range should shift up by 6 after deleting lines 0-5");
}

#[test]
fn test_tag_remap_after_insert_lines() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Create tag at lines 10-20
    app.tag_store.add_tag(&app.repo_name, lograph::tag::Tag {
        name: "middle".into(),
        ranges: vec![(10, 20)],
        created_at: chrono::Utc::now(),
    });

    // Insert 3 lines after line 5
    app.queue_operation(Operation::InsertLines {
        after_line: 5,
        content: vec!["a".into(), "b".into(), "c".into()],
    });
    app.apply_pending();

    // Tag range should shift down by 3
    let tags = app.tag_store.get_tags(&app.repo_name);
    assert!(!tags.is_empty());
    let tag = &tags[0];
    assert_eq!(tag.ranges[0], (13, 23), "Range should shift down by 3 after inserting 3 lines after line 5");
}

#[test]
fn test_tag_copy_duplicates_with_new_name() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    app.tag_store.add_tag(&app.repo_name, lograph::tag::Tag {
        name: "original".into(),
        ranges: vec![(5, 15)],
        created_at: chrono::Utc::now(),
    });

    // Simulate copy (y key in tag manager)
    let tags = app.tag_store.get_tags(&app.repo_name).to_vec();
    let tag = &tags[0];
    let new_name = app.tag_store.next_auto_name(&app.repo_name);
    app.tag_store.add_tag(&app.repo_name, lograph::tag::Tag {
        name: new_name,
        ranges: tag.ranges.clone(),
        created_at: chrono::Utc::now(),
    });

    let tags = app.tag_store.get_tags(&app.repo_name);
    assert_eq!(tags.len(), 2);
    // New tag should have tag_N name
    assert!(tags.iter().any(|t| t.name.starts_with("tag_")));
    assert!(tags.iter().any(|t| t.name == "original"));
}

#[test]
fn test_tag_replace_preserves_ranges() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Create tag
    app.tag_store.add_tag(&app.repo_name, lograph::tag::Tag {
        name: "keep-me".into(),
        ranges: vec![(10, 20)],
        created_at: chrono::Utc::now(),
    });

    // Replace (doesn't change line count)
    app.queue_operation(Operation::Replace {
        pattern: "ERROR".to_string(),
        replacement: "OK".to_string(),
    });
    app.apply_pending();

    let tags = app.tag_store.get_tags(&app.repo_name);
    assert!(!tags.is_empty());
    assert_eq!(tags[0].ranges, vec![(10, 20)], "Replace should preserve tag ranges");
}
