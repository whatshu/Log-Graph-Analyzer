/// Integration tests for HistoryTree in the context of LogRepo.
use log_analyzer_core::operator::Operation;
use log_analyzer_core::repo::LogRepo;
use std::fs;
use tempfile::TempDir;

fn make_test_log(lines: usize) -> String {
    (0..lines)
        .map(|i| {
            let level = match i % 4 {
                0 => "INFO",
                1 => "WARN",
                2 => "ERROR",
                3 => "DEBUG",
                _ => "unknown",
            };
            format!("2024-01-01 00:00:{:02} [{}] message {}", i % 60, level, i)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn import_temp_repo(tmp: &TempDir, name: &str) -> LogRepo {
    let log_file = tmp.path().join("test.log");
    let content = make_test_log(100);
    fs::write(&log_file, &content).unwrap();
    let repo_path = tmp.path().join(name);
    LogRepo::import(&repo_path, &log_file).unwrap()
}

// ── Tree structure basics ──

#[test]
fn test_new_repo_has_tree_with_root() {
    let tmp = TempDir::new().unwrap();
    let repo = import_temp_repo(&tmp, "repo");

    let tree = repo.history_tree();
    assert_eq!(tree.len(), 1); // root only
    assert_eq!(tree.head(), 0);
    assert_eq!(repo.current_branch(), "main");
    assert_eq!(repo.head_node_id(), 0);
}

#[test]
fn test_apply_operation_creates_nodes() {
    let tmp = TempDir::new().unwrap();
    let mut repo = import_temp_repo(&tmp, "repo");

    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();

    let tree = repo.history_tree();
    assert_eq!(tree.len(), 2); // root + 1 op
    assert_eq!(tree.head(), 1);
    assert_eq!(repo.head_node_id(), 1);

    repo.apply_operation(Operation::Filter {
        pattern: "message".to_string(),
        keep: true,
    })
    .unwrap();

    assert_eq!(repo.history_tree().len(), 3); // root + 2 ops
    assert_eq!(repo.head_node_id(), 2);
}

// ── Undo (non-destructive, moves branch HEAD back) ──

#[test]
fn test_undo_moves_head_back() {
    let tmp = TempDir::new().unwrap();
    let mut repo = import_temp_repo(&tmp, "repo");

    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();

    let _after_filter = repo.current_line_count().unwrap();

    repo.undo().unwrap();
    assert_eq!(repo.current_line_count().unwrap(), 100); // back to original

    // Tree still has the node (non-destructive undo)
    assert_eq!(repo.history_tree().len(), 2); // root + op node
    assert_eq!(repo.head_node_id(), 0); // HEAD moved back to root

    // Re-apply: can apply again from root (creates new child)
    repo.apply_operation(Operation::Filter {
        pattern: "WARN".to_string(),
        keep: true,
    })
    .unwrap();
    assert_eq!(repo.history_tree().len(), 3); // root now has 2 children
}

// ── Branching ──

#[test]
fn test_create_and_switch_branch() {
    let tmp = TempDir::new().unwrap();
    let mut repo = import_temp_repo(&tmp, "repo");

    // Apply an operation on main
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    let error_count = repo.current_line_count().unwrap();

    // Create branch at root and switch
    repo.branch_from("experiment", 0).unwrap();
    assert_eq!(repo.current_branch(), "experiment");
    assert_eq!(repo.current_line_count().unwrap(), 100); // root = original

    // Apply different operation on experiment
    repo.apply_operation(Operation::Replace {
        pattern: "ERROR".to_string(),
        replacement: "CRITICAL".to_string(),
    })
    .unwrap();
    let replace_count = repo.current_line_count().unwrap();
    assert_eq!(replace_count, 100); // replace doesn't change line count
    // Verify the content changed
    let lines = repo.get_current_lines().unwrap();
    assert!(lines.iter().any(|l| l.contains("CRITICAL")));

    // Switch back to main
    repo.checkout_branch("main").unwrap();
    assert_eq!(repo.current_branch(), "main");
    assert_eq!(repo.current_line_count().unwrap(), error_count);

    // Switch to experiment — has CRITICAL (was ERROR)
    repo.checkout_branch("experiment").unwrap();
    let exp_lines = repo.get_current_lines().unwrap();
    assert!(exp_lines.iter().any(|l| l.contains("CRITICAL")));
}

#[test]
fn test_branch_names() {
    let tmp = TempDir::new().unwrap();
    let mut repo = import_temp_repo(&tmp, "repo");

    repo.branch_from("dev", 0).unwrap();
    repo.checkout_branch("main").unwrap();
    repo.branch_from("staging", 0).unwrap();

    let names = repo.branch_names();
    assert!(names.contains(&"main"));
    assert!(names.contains(&"dev"));
    assert!(names.contains(&"staging"));
}

#[test]
fn test_delete_branch() {
    let tmp = TempDir::new().unwrap();
    let mut repo = import_temp_repo(&tmp, "repo");

    repo.branch_from("temp", 0).unwrap();
    repo.checkout_branch("main").unwrap();

    assert!(repo.delete_branch("temp").unwrap());
    let names = repo.branch_names();
    assert!(!names.contains(&"temp"));
}

#[test]
fn test_cannot_delete_main() {
    let tmp = TempDir::new().unwrap();
    let mut repo = import_temp_repo(&tmp, "repo");

    assert!(!repo.delete_branch("main").unwrap());
}

// ── Compute state at node ──

#[test]
fn test_compute_state_at_node() {
    let tmp = TempDir::new().unwrap();
    let mut repo = import_temp_repo(&tmp, "repo");

    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    let node1_id = repo.head_node_id();

    repo.apply_operation(Operation::Filter {
        pattern: "message 20".to_string(),
        keep: true,
    })
    .unwrap();
    let node2_id = repo.head_node_id();

    // Compute state at root (0) — should be original 100 lines
    let state_root = repo.compute_state_at(0).unwrap();
    assert_eq!(state_root.len(), 100);

    // Compute state at first operation
    let state_node1 = repo.compute_state_at(node1_id).unwrap();
    assert!(state_node1.iter().all(|l| l.contains("ERROR")));

    // Compute state at second operation
    let state_node2 = repo.compute_state_at(node2_id).unwrap();
    assert!(state_node2.iter().all(|l| l.contains("ERROR") && l.contains("message 20")));
}

#[test]
fn test_line_count_at_node() {
    let tmp = TempDir::new().unwrap();
    let mut repo = import_temp_repo(&tmp, "repo");

    assert_eq!(repo.line_count_at(0).unwrap(), 100);

    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    let node_id = repo.head_node_id();
    assert!(repo.line_count_at(node_id).unwrap() < 100);
}

// ── Apply operation from specific node ──

#[test]
fn test_apply_operation_from_node() {
    let tmp = TempDir::new().unwrap();
    let mut repo = import_temp_repo(&tmp, "repo");

    // Apply on main
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();

    // Branch from root and apply a different operation
    repo.apply_operation_from(
        0,
        "warn-branch",
        Operation::Filter {
            pattern: "WARN".to_string(),
            keep: true,
        },
    )
    .unwrap();

    assert_eq!(repo.current_branch(), "warn-branch");
    let lines = repo.get_current_lines().unwrap();
    assert!(lines.iter().all(|l| l.contains("WARN")));
    assert!(!lines.iter().any(|l| l.contains("ERROR")));

    // Main still has ERROR filter
    repo.checkout_branch("main").unwrap();
    let main_lines = repo.get_current_lines().unwrap();
    assert!(main_lines.iter().all(|l| l.contains("ERROR")));
}

// ── Persistence ──

#[test]
fn test_tree_persists_across_reopen() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("persist-repo");

    {
        let mut repo = import_temp_repo(&tmp, "persist-repo");
        repo.apply_operation(Operation::Filter {
            pattern: "ERROR".to_string(),
            keep: true,
        })
        .unwrap();
        repo.branch_from("dev", 0).unwrap();
        repo.checkout_branch("main").unwrap();
    }

    // Reopen
    let repo = LogRepo::open(&repo_path).unwrap();
    let tree = repo.history_tree();
    assert_eq!(tree.len(), 2); // root + 1 op
    assert_eq!(repo.branch_names().len(), 2); // main + dev
    assert!(repo.branch_names().contains(&"main"));
    assert!(repo.branch_names().contains(&"dev"));
}

// ── History records for Python bindings ──

#[test]
fn test_history_records() {
    let tmp = TempDir::new().unwrap();
    let mut repo = import_temp_repo(&tmp, "repo");

    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();

    repo.apply_operation(Operation::Replace {
        pattern: "ERROR".to_string(),
        replacement: "CRITICAL".to_string(),
    })
    .unwrap();

    let records = repo.history_records();
    assert_eq!(records.len(), 2);
    // IDs start at 1 (root is node 0)
    assert_eq!(records[0].id, 1);
    assert_eq!(records[1].id, 2);
    assert!(records[0].operation.describe().contains("filter"));
    assert!(records[1].operation.describe().contains("replace"));
}
