//! Tests for [`super::handlers`] — input handling and command parsing.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use lograph::operator::Operation;
use tempfile::TempDir;

use crate::tui::app::ViewKind;
use crate::tui::handlers::commands::{execute_collect, execute_command};
use crate::tui::handlers::normal_mode::normal_mode;
use crate::tui::tests::test_utils::setup_app;

// ── execute_collect parsing tests ──

#[test]
fn test_execute_collect_count_no_pattern() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    execute_collect(&mut app, "count");
    assert!(app.show_collect_detail);
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Count: 200 lines"));
}

#[test]
fn test_execute_collect_count_with_pattern() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    execute_collect(&mut app, "count ERROR");
    assert!(app.show_collect_detail);
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Count: 50 lines"));
}

#[test]
fn test_execute_collect_group_count() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    execute_collect(&mut app, r"group \[(\w+)\] 1");
    assert!(app.show_collect_detail);
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Group Count"));
    assert!(detail.contains("INFO"));
    assert!(detail.contains("ERROR"));
}

#[test]
fn test_execute_collect_topn() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    execute_collect(&mut app, r"topn \[(\w+)\] 1 3");
    assert!(app.show_collect_detail);
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Top-3") || detail.contains("Top-4"));
}

#[test]
fn test_execute_collect_unique() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    execute_collect(&mut app, r"unique \[(\w+)\] 1");
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("4 distinct"));
}

#[test]
fn test_execute_collect_numstats() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    execute_collect(&mut app, r"numstats message (\d+) 1");
    assert!(app.show_collect_detail);
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Numeric Statistics"));
}

#[test]
fn test_execute_collect_linestats() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    execute_collect(&mut app, "linestats");
    assert!(app.show_collect_detail);
    let detail = app.collect_detail.as_ref().unwrap();
    assert!(detail.contains("Line Statistics"));
}

#[test]
fn test_execute_collect_unknown_type_error() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    execute_collect(&mut app, "bogus args");
    assert!(!app.show_collect_detail);
    assert!(app.error_message.is_some());
    assert!(app.error_message.as_ref().unwrap().contains("Unknown collect type"));
}

#[test]
fn test_execute_collect_missing_pattern_error() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // group without pattern should error
    execute_collect(&mut app, "group");
    assert!(!app.show_collect_detail);
    assert!(app.error_message.is_some());
    assert!(app.error_message.as_ref().unwrap().contains("Usage: group"));
}

#[test]
fn test_execute_collect_topn_with_defaults() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Only provide pattern, group_index and n should default
    execute_collect(&mut app, r"topn \[(\w+)\]");
    assert!(app.show_collect_detail);
    let detail = app.collect_detail.as_ref().unwrap();
    // Default n=10, group_index=1
    assert!(detail.contains("Top-"));
}

#[test]
fn test_collect_command_prefix_routing() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Simulate what happens when user types ":collect count"
    execute_command(&mut app, "collect count");
    assert!(app.show_collect_detail);
    assert!(app.collect_detail.as_ref().unwrap().contains("Count: 200 lines"));
}

#[test]
fn test_collect_command_prefix_with_args() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // With extra whitespace
    execute_command(&mut app, "collect   count   ERROR");
    assert!(app.show_collect_detail);
    assert!(app.collect_detail.as_ref().unwrap().contains("Count: 50 lines"));
}

#[test]
fn test_collect_direct_count_command() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Direct :count command (without collect prefix)
    execute_command(&mut app, "count ERROR");
    assert!(app.show_collect_detail);
    assert!(app.collect_detail.as_ref().unwrap().contains("Count: 50 lines"));
}

#[test]
fn test_collect_direct_linestats_command() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    execute_command(&mut app, "linestats");
    assert!(app.show_collect_detail);
    assert!(app.collect_detail.as_ref().unwrap().contains("Line Statistics"));
}

// ── History view Enter resolves node ID from cursor index ──

#[test]
fn test_history_enter_uses_resolved_node_id() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Create multiple nodes.
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
    app.active_view = ViewKind::History;

    // history_nodes = [node0(import), node1(filter ERROR), node2(filter message0)]
    // HEAD is node2 at cursor index 2. Move cursor to a specific position.
    app.history_cursor = 1;
    // The key fix: cursor is an array INDEX, not a node ID.
    // Resolve to actual node ID from history_nodes.
    let expected_node_id = app.history_nodes[app.history_cursor].id;

    // Simulate pressing Enter in history view
    normal_mode(
        &mut app,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );

    // The pending op must contain the resolved node ID from history_nodes,
    // NOT the raw cursor index.
    match &app.pending_op {
        crate::tui::app::PendingOp::CheckoutTo(id) => {
            assert_eq!(*id, expected_node_id,
                "CheckoutTo must store the resolved node ID from history_nodes[cursor].id");
        }
        other => panic!("Expected CheckoutTo after Enter in history view, got {:?}", other),
    }
}

#[test]
fn test_history_enter_all_cursor_positions_use_resolved_node_id() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Create nodes using apply_operation_from to build a non-trivial tree.
    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();

    // Branch from node 0.
    app.queue_operation_from(0, Operation::Filter {
        pattern: "DEBUG".to_string(),
        keep: true,
    });
    app.apply_pending();

    // Continue on current branch.
    app.queue_operation(Operation::Filter {
        pattern: "INFO".to_string(),
        keep: true,
    });
    app.apply_pending();

    app.build_history();
    app.active_view = ViewKind::History;

    assert!(app.history_nodes.len() >= 3,
        "Need at least 3 history nodes for a meaningful test");

    // For every cursor position, simulate pressing Enter and verify
    // CheckoutTo contains the resolved node ID from history_nodes,
    // NOT the raw cursor index. Reset active_view to History before
    // each call since Enter now switches to LogView.
    for cursor in 0..app.history_nodes.len() {
        app.active_view = ViewKind::History;
        app.history_cursor = cursor;
        let expected_id = app.history_nodes[cursor].id;

        normal_mode(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        match &app.pending_op {
            crate::tui::app::PendingOp::CheckoutTo(id) => {
                assert_eq!(*id, expected_id,
                    "Enter on history_nodes[{}] (id={}) should checkout to id={}, got id={}",
                    cursor, expected_id, expected_id, id);
            }
            other => panic!(
                "Enter on history_nodes[{}] expected CheckoutTo, got {:?}",
                cursor, other
            ),
        }
    }
}

#[test]
fn test_history_enter_returns_to_head_when_head_selected() {
    let tmp = TempDir::new().unwrap();
    let mut app = setup_app(&tmp);

    // Apply one operation to have a non-root HEAD
    app.queue_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    });
    app.apply_pending();

    app.build_history();
    app.active_view = ViewKind::History;

    // Find cursor position of HEAD node
    let head_id = app.repo.borrow().as_ref().unwrap().head_node_id();
    let head_cursor = app.history_nodes.iter().position(|n| n.id == head_id).unwrap();
    app.history_cursor = head_cursor;

    normal_mode(
        &mut app,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );

    match &app.pending_op {
        crate::tui::app::PendingOp::CheckoutTo(id) => {
            assert_eq!(*id, head_id, "Enter on HEAD should checkout to HEAD node ID");
        }
        other => panic!("Expected CheckoutTo after Enter on HEAD, got {:?}", other),
    }
}
