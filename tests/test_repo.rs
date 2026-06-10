use std::fs;
use tempfile::TempDir;

use lograph::operator::{MergeMode, Operation};
use lograph::repo::LogRepo;

fn create_test_log(lines: usize) -> String {
    (0..lines)
        .map(|i| {
            let level = match i % 4 {
                0 => "INFO",
                1 => "WARN",
                2 => "ERROR",
                3 => "DEBUG",
                _ => unreachable!(),
            };
            format!(
                "2024-01-{:02} {:02}:{:02}:{:02} {} [thread-{}] message number {}",
                (i % 28) + 1,
                i % 24,
                i % 60,
                i % 60,
                level,
                i % 8,
                i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_import_and_read() {
    let tmp = TempDir::new().unwrap();
    let log_file = tmp.path().join("test.log");
    let repo_path = tmp.path().join("repo");

    let content = "line0\nline1\nline2\nline3\nline4\n";
    fs::write(&log_file, content).unwrap();

    let repo = LogRepo::import(&repo_path, &log_file).unwrap();
    assert_eq!(repo.original_line_count(), 5);

    let line = repo.read_original_line(0).unwrap();
    assert_eq!(line, "line0");

    let line = repo.read_original_line(4).unwrap();
    assert_eq!(line, "line4");
}

#[test]
fn test_import_from_bytes() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"hello\nworld\n";
    let repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    assert_eq!(repo.original_line_count(), 2);
    assert_eq!(repo.read_original_line(0).unwrap(), "hello");
    assert_eq!(repo.read_original_line(1).unwrap(), "world");
}

#[test]
fn test_open_existing_repo() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"line0\nline1\nline2\n";
    LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Re-open
    let repo = LogRepo::open(&repo_path).unwrap();
    assert_eq!(repo.original_line_count(), 3);
    assert_eq!(repo.read_original_line(1).unwrap(), "line1");
}

#[test]
fn test_clone_repo() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let clone_path = tmp.path().join("repo_clone");

    let data = b"a\nb\nc\n";
    let repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    let cloned = repo.clone_to(&clone_path).unwrap();
    assert_eq!(cloned.original_line_count(), 3);
    assert_eq!(cloned.read_original_line(0).unwrap(), "a");
}

#[test]
fn test_filter_operation() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let log_data = create_test_log(100);
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, log_data.as_bytes(), "test.log".into()).unwrap();
    assert_eq!(repo.original_line_count(), 100);

    // Filter to keep only ERROR lines
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();

    let count = repo.current_line_count().unwrap();
    assert_eq!(count, 25); // Every 4th line is ERROR

    let lines = repo.get_current_lines().unwrap();
    for line in &lines {
        assert!(line.contains("ERROR"));
    }
}

#[test]
fn test_replace_operation() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"hello world\nfoo bar\nhello foo\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    repo.apply_operation(Operation::Replace {
        pattern: "hello".to_string(),
        replacement: "HI".to_string(),
    })
    .unwrap();

    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines[0], "HI world");
    assert_eq!(lines[1], "foo bar");
    assert_eq!(lines[2], "HI foo");
}

#[test]
fn test_delete_lines_operation() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\nd\ne\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    repo.apply_operation(Operation::DeleteLines {
        line_indices: vec![1, 3],
    })
    .unwrap();

    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines, vec!["a", "c", "e"]);
}

#[test]
fn test_insert_lines_operation() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    repo.apply_operation(Operation::InsertLines {
        after_line: 1,
        content: vec!["x".to_string(), "y".to_string()],
    })
    .unwrap();

    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines, vec!["a", "x", "y", "b", "c"]);
}

#[test]
fn test_modify_line_operation() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    repo.apply_operation(Operation::ModifyLine {
        line_index: 1,
        new_content: "modified".to_string(),
    })
    .unwrap();

    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines, vec!["a", "modified", "c"]);
}

#[test]
fn test_undo_filter() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"keep_a\nremove_b\nkeep_c\nremove_d\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Apply filter
    repo.apply_operation(Operation::Filter {
        pattern: "keep".to_string(),
        keep: true,
    })
    .unwrap();
    assert_eq!(repo.current_line_count().unwrap(), 2);

    // Undo
    repo.undo().unwrap();
    assert_eq!(repo.current_line_count().unwrap(), 4);

    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines, vec!["keep_a", "remove_b", "keep_c", "remove_d"]);
}

#[test]
fn test_undo_replace() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"hello world\nfoo bar\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    repo.apply_operation(Operation::Replace {
        pattern: "hello".to_string(),
        replacement: "HI".to_string(),
    })
    .unwrap();

    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines[0], "HI world");

    repo.undo().unwrap();
    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines[0], "hello world");
}

#[test]
fn test_undo_delete() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    repo.apply_operation(Operation::DeleteLines {
        line_indices: vec![1],
    })
    .unwrap();
    assert_eq!(repo.current_line_count().unwrap(), 2);

    repo.undo().unwrap();
    assert_eq!(repo.current_line_count().unwrap(), 3);

    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines, vec!["a", "b", "c"]);
}

#[test]
fn test_multiple_operations_and_undo() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let log_data = create_test_log(50);
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, log_data.as_bytes(), "test.log".into()).unwrap();

    let original_count = repo.original_line_count();

    // Op 1: filter to ERROR
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    let after_filter = repo.current_line_count().unwrap();

    // Op 2: replace timestamps
    repo.apply_operation(Operation::Replace {
        pattern: r"\d{4}-\d{2}-\d{2}".to_string(),
        replacement: "DATE".to_string(),
    })
    .unwrap();

    // Check history (tree has root + 2 ops = 3 nodes; history_tree includes root)
    assert_eq!(repo.history_tree().len(), 3);

    // Undo replace
    repo.undo().unwrap();
    // Nodes are never deleted — tree still has 3 nodes, but HEAD moved back
    assert_eq!(repo.history_tree().len(), 3);
    assert_eq!(repo.current_line_count().unwrap(), after_filter);

    // Undo filter
    repo.undo().unwrap();
    // HEAD back to root — 3 nodes still exist
    assert_eq!(repo.history_tree().len(), 3);
    assert_eq!(repo.current_line_count().unwrap(), original_count);
}

#[test]
fn test_export() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let export_path = tmp.path().join("exported.log");

    let data = b"line1\nline2\nline3\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Apply a filter
    repo.apply_operation(Operation::Filter {
        pattern: "line[12]".to_string(),
        keep: true,
    })
    .unwrap();

    repo.export(&export_path).unwrap();

    let exported = fs::read_to_string(&export_path).unwrap();
    assert_eq!(exported, "line1\nline2");
}

#[test]
fn test_operations_persist_across_reopen() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\nd\ne\n";
    {
        let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();
        repo.apply_operation(Operation::Filter {
            pattern: "[ace]".to_string(),
            keep: true,
        })
        .unwrap();
    }

    // Reopen and verify operations are preserved
    let mut repo = LogRepo::open(&repo_path).unwrap();
    assert_eq!(repo.history_tree().len(), 2); // root + 1 op

    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines, vec!["a", "c", "e"]);
}

#[test]
fn test_metadata() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"hello\nworld\n";
    let repo = LogRepo::import_from_bytes(&repo_path, data, "myfile.log".into()).unwrap();

    assert_eq!(repo.metadata.source_name, "myfile.log");
    assert_eq!(repo.metadata.original_size, 12);
    assert_eq!(repo.metadata.original_line_count, 2);
    assert!(!repo.metadata.id.is_empty());
}

#[test]
fn test_large_log_chunking() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    // Create a log with enough lines to span multiple chunks
    let log_data = create_test_log(25_000);
    let repo =
        LogRepo::import_from_bytes(&repo_path, log_data.as_bytes(), "large.log".into()).unwrap();

    assert_eq!(repo.original_line_count(), 25_000);

    // Verify random access works across chunks
    let line_0 = repo.read_original_line(0).unwrap();
    assert!(line_0.contains("message number 0"));

    let line_9999 = repo.read_original_line(9_999).unwrap();
    assert!(line_9999.contains("message number 9999"));

    let line_15000 = repo.read_original_line(15_000).unwrap();
    assert!(line_15000.contains("message number 15000"));

    let line_last = repo.read_original_line(24_999).unwrap();
    assert!(line_last.contains("message number 24999"));
}

#[test]
fn test_read_range() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"line0\nline1\nline2\nline3\nline4\n";
    let repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    let lines = repo.read_original_lines(1, 3).unwrap();
    assert_eq!(lines, vec!["line1", "line2", "line3"]);
}

// -------- Append tests --------

#[test]
fn test_append_basic() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();
    assert_eq!(repo.original_line_count(), 3);

    let added = repo.append_bytes(b"d\ne\n").unwrap();
    assert_eq!(added, 2);
    assert_eq!(repo.original_line_count(), 5);

    assert_eq!(repo.read_original_line(0).unwrap(), "a");
    assert_eq!(repo.read_original_line(3).unwrap(), "d");
    assert_eq!(repo.read_original_line(4).unwrap(), "e");
}

#[test]
fn test_append_multiple_times() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let mut repo =
        LogRepo::import_from_bytes(&repo_path, b"line0\n", "test".into()).unwrap();

    repo.append_bytes(b"line1\nline2\n").unwrap();
    repo.append_bytes(b"line3\n").unwrap();

    assert_eq!(repo.original_line_count(), 4);
    let lines = repo.read_all_original_lines().unwrap();
    assert_eq!(lines, vec!["line0", "line1", "line2", "line3"]);
}

#[test]
fn test_append_empty() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let mut repo =
        LogRepo::import_from_bytes(&repo_path, b"a\n", "test".into()).unwrap();

    let added = repo.append_bytes(b"").unwrap();
    assert_eq!(added, 0);
    assert_eq!(repo.original_line_count(), 1);
}

#[test]
fn test_append_preserves_operations() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let mut repo =
        LogRepo::import_from_bytes(&repo_path, b"INFO a\nERROR b\n", "test".into()).unwrap();

    // Apply a filter
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    assert_eq!(repo.current_line_count().unwrap(), 1);

    // Append more data — operations re-apply over all data
    repo.append_bytes(b"INFO c\nERROR d\n").unwrap();
    assert_eq!(repo.original_line_count(), 4);

    // Current state = filter applied to all 4 lines
    let current = repo.get_current_lines().unwrap();
    assert_eq!(current, vec!["ERROR b", "ERROR d"]);
}

#[test]
fn test_append_persists_across_reopen() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    {
        let mut repo =
            LogRepo::import_from_bytes(&repo_path, b"x\ny\n", "test".into()).unwrap();
        repo.append_bytes(b"z\n").unwrap();
    }

    let repo = LogRepo::open(&repo_path).unwrap();
    assert_eq!(repo.original_line_count(), 3);
    assert_eq!(repo.read_original_line(2).unwrap(), "z");
}

#[test]
fn test_append_file() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let extra_file = tmp.path().join("extra.log");

    let mut repo =
        LogRepo::import_from_bytes(&repo_path, b"first\n", "test".into()).unwrap();

    fs::write(&extra_file, "second\nthird\n").unwrap();
    let added = repo.append_file(&extra_file).unwrap();
    assert_eq!(added, 2);
    assert_eq!(repo.original_line_count(), 3);

    let lines = repo.read_all_original_lines().unwrap();
    assert_eq!(lines, vec!["first", "second", "third"]);
}

#[test]
fn test_append_large_across_chunks() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let first_batch = create_test_log(15_000);
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, first_batch.as_bytes(), "batch1".into()).unwrap();
    assert_eq!(repo.original_line_count(), 15_000);

    let second_batch = create_test_log(10_000);
    let added = repo.append_bytes(second_batch.as_bytes()).unwrap();
    assert_eq!(added, 10_000);
    assert_eq!(repo.original_line_count(), 25_000);

    // Verify we can read lines from both batches
    let line_0 = repo.read_original_line(0).unwrap();
    assert!(line_0.contains("message number 0"));

    let line_14999 = repo.read_original_line(14_999).unwrap();
    assert!(line_14999.contains("message number 14999"));

    // Lines from second batch
    let line_15000 = repo.read_original_line(15_000).unwrap();
    assert!(line_15000.contains("message number 0"));

    let line_last = repo.read_original_line(24_999).unwrap();
    assert!(line_last.contains("message number 9999"));
}

#[test]
fn test_append_metadata_updated() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data1 = b"hello\n";
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, data1, "test".into()).unwrap();
    let size_before = repo.metadata.original_size;

    let data2 = b"world\n";
    repo.append_bytes(data2).unwrap();

    assert_eq!(repo.metadata.original_size, size_before + data2.len() as u64);
    assert_eq!(repo.metadata.original_line_count, 2);
}

// ── Branch operation tests ──

#[test]
fn test_branch_create_and_checkout() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\nd\ne\n";
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Apply a filter operation first (creates node 1)
    repo.apply_operation(Operation::Filter {
        pattern: "[ace]".to_string(),
        keep: true,
    })
    .unwrap();
    assert_eq!(repo.current_line_count().unwrap(), 3);

    // Create a new branch at node 0 (root)
    let created = repo.create_branch("experiment", 0).unwrap();
    assert!(created);

    // Checkout to the new branch
    repo.checkout_branch("experiment").unwrap();
    assert_eq!(repo.current_branch(), "experiment");
    // experiment branch HEAD is at node 0 (root) — all original lines
    assert_eq!(repo.current_line_count().unwrap(), 5);
}

#[test]
fn test_branch_names_and_head() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"line1\nline2\nline3\n";
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Initially only "main" exists
    let names = repo.branch_names();
    assert!(names.contains(&"main"));
    assert_eq!(repo.current_branch(), "main");

    // Head should be at root (node 0)
    assert_eq!(repo.head_node_id(), 0);

    // Create branch at root
    repo.create_branch("alt", 0).unwrap();
    let names = repo.branch_names();
    assert!(names.contains(&"alt"));
    assert_eq!(repo.branch_head_node_id("alt"), Some(0));
}

#[test]
fn test_branch_delete() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\n";
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    repo.create_branch("temp", 0).unwrap();
    assert!(repo.branch_names().contains(&"temp"));

    // Switch away first
    repo.checkout_branch("temp").unwrap();
    repo.checkout_branch("main").unwrap();

    let deleted = repo.delete_branch("temp").unwrap();
    assert!(deleted);
    assert!(!repo.branch_names().contains(&"temp"));
}

#[test]
fn test_branch_cannot_delete_main() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\n";
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    let deleted = repo.delete_branch("main").unwrap();
    assert!(!deleted);
}

#[test]
fn test_view_node() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\nd\ne\n";
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Apply filter
    repo.apply_operation(Operation::Filter {
        pattern: "[ace]".to_string(),
        keep: true,
    })
    .unwrap();

    // View root node (original state) — non-destructive
    let root_lines = repo.view_node(0).unwrap();
    assert_eq!(root_lines.len(), 5);
    assert_eq!(root_lines, vec!["a", "b", "c", "d", "e"]);

    // Current branch HEAD should still be node 1 (filtered state)
    assert_eq!(repo.head_node_id(), 1);
    assert_eq!(repo.current_line_count().unwrap(), 3);
}

#[test]
fn test_branch_from() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"INFO a\nERROR b\nINFO c\nERROR d\n";
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Apply a filter on main
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    assert_eq!(repo.current_line_count().unwrap(), 2);

    // Branch from root (node 0) with a different operation
    repo.branch_from("info_only", 0).unwrap();
    assert_eq!(repo.current_branch(), "info_only");
    assert_eq!(repo.current_line_count().unwrap(), 4); // root state

    // Apply different filter on new branch
    repo.apply_operation(Operation::Filter {
        pattern: "INFO".to_string(),
        keep: true,
    })
    .unwrap();
    assert_eq!(repo.current_line_count().unwrap(), 2);

    // Switch back to main — it should still have ERROR filter applied
    repo.checkout_branch("main").unwrap();
    let main_lines = repo.get_current_lines().unwrap();
    assert!(main_lines.iter().all(|l| l.contains("ERROR")));
}

#[test]
fn test_collect_original() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"INFO ok\nERROR fail\nINFO ok2\nERROR oops\n";
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Apply filter to only keep ERROR lines
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();

    // Current state has 2 lines (only ERROR)
    assert_eq!(repo.current_line_count().unwrap(), 2);

    // collect_original should see all 4 lines
    use lograph::engine::{CollectResult, Collector};
    let result = repo
        .collect_original(&Collector::Count { pattern: None })
        .unwrap();
    assert!(matches!(result, CollectResult::Count(4)));

    // Current state collector should see 2 lines
    let current_result = repo
        .collect(&Collector::Count { pattern: None })
        .unwrap();
    assert!(matches!(current_result, CollectResult::Count(2)));
}

#[test]
fn test_history_tree_node_count() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"a\nb\nc\n";
    let mut repo =
        LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    assert_eq!(repo.history_tree().len(), 1); // just root
    assert!(repo.history_tree().is_empty());

    repo.apply_operation(Operation::Filter {
        pattern: "a".to_string(),
        keep: true,
    })
    .unwrap();
    assert_eq!(repo.history_tree().len(), 2);
    assert!(!repo.history_tree().is_empty());

    repo.apply_operation(Operation::Replace {
        pattern: "a".to_string(),
        replacement: "X".to_string(),
    })
    .unwrap();
    assert_eq!(repo.history_tree().len(), 3);
}

// ── Tag-scoped operations ──

#[test]
fn test_apply_operation_with_tag_scope() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"INFO msg1\nERROR msg2\nWARN msg3\nERROR msg4\nDEBUG msg5\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    let scope = lograph::tag::TagScopeRef {
        tag_name: "errors".into(),
        ranges: vec![(1, 3)], // Lines 1-3 (0-based): ERROR msg2, WARN msg3, ERROR msg4
    };

    // Filter keep ERROR within the scope — only lines 1-3 are considered
    repo.apply_operation_scoped(
        Operation::Filter {
            pattern: "ERROR".to_string(),
            keep: true,
        },
        Some(scope.clone()),
    )
    .unwrap();

    let lines = repo.get_current_lines().unwrap();
    // Lines outside scope (0, 4) pass through unchanged
    // Within scope: ERROR msg2 kept, WARN msg3 removed, ERROR msg4 kept
    assert_eq!(lines, vec!["INFO msg1", "ERROR msg2", "ERROR msg4", "DEBUG msg5"]);
    assert_eq!(lines.len(), 4);

    // Verify tag scope was recorded on the history node
    let tree = repo.history_tree();
    let head_node = tree.get_node(tree.head()).unwrap();
    assert!(head_node.tag_scope.is_some());
    assert_eq!(head_node.tag_scope.as_ref().unwrap().tag_name, "errors");
}

#[test]
fn test_apply_operation_scoped_outside_lines_preserved() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"A\nB\nC\nD\nE\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    let scope = lograph::tag::TagScopeRef {
        tag_name: "middle".into(),
        ranges: vec![(1, 2)], // Only lines B and C
    };

    // Filter keep everything within scope (no-op on scoped lines)
    repo.apply_operation_scoped(
        Operation::Filter {
            pattern: ".".to_string(),
            keep: true,
        },
        Some(scope),
    )
    .unwrap();

    let lines = repo.get_current_lines().unwrap();
    // All lines should be preserved since filter keeps everything
    assert_eq!(lines, vec!["A", "B", "C", "D", "E"]);
}

#[test]
fn test_apply_operation_multiple_ranges_in_scope() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"0:A\n1:B\n2:C\n3:D\n4:E\n5:F\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    let scope = lograph::tag::TagScopeRef {
        tag_name: "multi".into(),
        ranges: vec![(0, 1), (4, 5)], // Lines 0-1 and 4-5
    };

    // Filter remove lines containing "A" or "E" (only within scope)
    repo.apply_operation_scoped(
        Operation::Filter {
            pattern: "[AE]".to_string(),
            keep: false,
        },
        Some(scope),
    )
    .unwrap();

    let lines = repo.get_current_lines().unwrap();
    // The HashSet approach walks original lines in order:
    // line 0 in scope → scoped[0]="1:B", line 1 in scope → scoped[1]="5:F" → no wait...
    // Actually the scoped filter removes [AE], so scoped lines are ["1:B", "5:F"]
    // Processing original lines 0-5:
    // 0 in scope → push "1:B", 1 in scope → push "5:F",
    // 2 not in scope → push "2:C", 3 not in scope → "3:D",
    // 4 in scope → skip (no more scoped results), 5 in scope → skip
    assert_eq!(lines, vec!["1:B", "5:F", "2:C", "3:D"]);
}

// ── Node merge (OR union) ──

#[test]
fn test_merge_nodes_union() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"ERROR auth\nWARN memory\nERROR disk\nINFO startup\nDEBUG trace\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR lines → "ERROR auth", "ERROR disk"
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();

    // Node 2: keep WARN lines (from original, not from node 1)
    // First undo
    repo.undo().unwrap();

    // Now from original, create a branch for WARN
    repo.apply_operation_from(0, "warn-branch", Operation::Filter {
        pattern: "WARN".to_string(),
        keep: true,
    })
    .unwrap();

    // Merge node 1 and node 2
    let new_id = repo.merge_nodes(&[1, 2], "merged", MergeMode::Union).unwrap();

    let lines = repo.get_current_lines().unwrap();
    // Union of ["ERROR auth", "ERROR disk"] and ["WARN memory"]
    // Should contain 3 unique lines
    assert_eq!(lines.len(), 3);
    assert!(lines.contains(&"ERROR auth".to_string()));
    assert!(lines.contains(&"ERROR disk".to_string()));
    assert!(lines.contains(&"WARN memory".to_string()));

    // Verify operation type on the new node
    let new_node = repo.history_tree().get_node(new_id).unwrap();
    match new_node.operation.as_ref().unwrap() {
        Operation::Merge { sources, mode } => {
            assert_eq!(sources.len(), 2);
            assert!(matches!(mode, MergeMode::Union));
        }
        _ => panic!("Expected Merge operation"),
    }
}

#[test]
fn test_merge_nodes_single_source() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"A\nB\nC\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    repo.apply_operation(Operation::Filter {
        pattern: "[AB]".to_string(),
        keep: true,
    })
    .unwrap();

    let new_id = repo.merge_nodes(&[1], "single-merge", MergeMode::Union).unwrap();
    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines, vec!["A", "B"]);
}

// ── Node merge intersection (AND) ──

#[test]
fn test_merge_nodes_intersection() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let data = b"ERROR auth\nWARN memory\nERROR disk\nINFO startup\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR → "ERROR auth", "ERROR disk"
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep "disk" → "ERROR disk"
    repo.apply_operation_from(0, "disk-branch", Operation::Filter {
        pattern: "disk".to_string(),
        keep: true,
    })
    .unwrap();

    // Intersection of node 1 and node 2: only "ERROR disk"
    let new_id = repo.merge_nodes(&[1, 2], "and-merge", MergeMode::Intersection).unwrap();
    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], "ERROR disk");

    let new_node = repo.history_tree().get_node(new_id).unwrap();
    match new_node.operation.as_ref().unwrap() {
        Operation::Merge { sources, mode } => {
            assert_eq!(sources.len(), 2);
            assert!(matches!(mode, MergeMode::Intersection));
        }
        _ => panic!("Expected Merge operation"),
    }
}

// ── Node merge subtract (SUB) ──

#[test]
fn test_merge_nodes_subtract() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let data = b"ERROR auth\nERROR disk\nWARN memory\nINFO startup\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR → "ERROR auth", "ERROR disk"
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep "disk" → "ERROR disk"
    repo.apply_operation_from(0, "disk-branch", Operation::Filter {
        pattern: "disk".to_string(),
        keep: true,
    })
    .unwrap();

    // SUB: node1 minus node2 → "ERROR auth"
    let new_id = repo.merge_nodes(&[1, 2], "sub-merge", MergeMode::Subtract).unwrap();
    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], "ERROR auth");

    let new_node = repo.history_tree().get_node(new_id).unwrap();
    match new_node.operation.as_ref().unwrap() {
        Operation::Merge { sources, mode } => {
            assert!(matches!(mode, MergeMode::Subtract));
        }
        _ => panic!("Expected Merge operation"),
    }
}

// ── Node merge symmetric difference (XOR) ──

#[test]
fn test_merge_nodes_xor() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let data = b"ERROR auth\nWARN memory\nERROR disk\nINFO startup\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR → "ERROR auth", "ERROR disk"
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep "disk" and "INFO" → "ERROR disk", "INFO startup"
    repo.apply_operation_from(0, "other-branch", Operation::Filter {
        pattern: "disk|INFO".to_string(),
        keep: true,
    })
    .unwrap();

    // XOR: node1 xor node2
    // node1: ERROR auth, ERROR disk
    // node2: ERROR disk, INFO startup
    // XOR: ERROR auth (1x), INFO startup (1x), ERROR disk (2x → excluded)
    let new_id = repo.merge_nodes(&[1, 2], "xor-merge", MergeMode::Xor).unwrap();
    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines.len(), 2);
    assert!(lines.contains(&"ERROR auth".to_string()));
    assert!(lines.contains(&"INFO startup".to_string()));

    let new_node = repo.history_tree().get_node(new_id).unwrap();
    match new_node.operation.as_ref().unwrap() {
        Operation::Merge { sources, mode } => {
            assert!(matches!(mode, MergeMode::Xor));
        }
        _ => panic!("Expected Merge operation"),
    }
}

// ── Node merge three sources ──

#[test]
fn test_merge_nodes_three_sources_union() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let data = b"A\nB\nC\nD\nE\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep A, B
    repo.apply_operation(Operation::Filter {
        pattern: "[AB]".into(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep C, D
    repo.apply_operation_from(0, "cd", Operation::Filter {
        pattern: "[CD]".into(),
        keep: true,
    })
    .unwrap();

    // Node 3: keep E (from another branch)
    repo.checkout_branch("main").unwrap();
    repo.apply_operation_from(0, "e", Operation::Filter {
        pattern: "E".into(),
        keep: true,
    })
    .unwrap();

    let new_id = repo.merge_nodes(&[1, 2, 3], "three-union", MergeMode::Union).unwrap();
    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines.len(), 5);
    for ch in &["A", "B", "C", "D", "E"] {
        assert!(lines.contains(&ch.to_string()));
    }
}

// ── Node merge LCA parent attachment ──

#[test]
fn test_merge_nodes_lca_parent() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let data = b"ERROR auth\nWARN memory\nERROR disk\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR (child of root)
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();

    // Create a branch from node 1, apply another filter → node 2 is child of node 1
    repo.create_branch("branch-a", 1).unwrap();
    repo.checkout_branch("branch-a").unwrap();
    repo.apply_operation(Operation::Filter {
        pattern: "auth".to_string(),
        keep: true,
    })
    .unwrap();
    // Now node 2 is a child of node 1

    // Create another branch from node 1, apply different filter → node 3
    repo.create_branch("branch-b", 1).unwrap();
    repo.checkout_branch("branch-b").unwrap();
    repo.apply_operation(Operation::Filter {
        pattern: "disk".to_string(),
        keep: true,
    })
    .unwrap();
    // Now node 3 is also a child of node 1

    // LCA of nodes 2 and 3 should be node 1 (not root)
    let lca = repo.history_tree().lowest_common_ancestor(&[2, 3]).unwrap();
    assert_eq!(lca, 1);

    // Merge nodes 2 and 3 — the new node should be attached to the LCA (node 1)
    let new_id = repo.merge_nodes(&[2, 3], "lca-merge", MergeMode::Union).unwrap();
    let new_node = repo.history_tree().get_node(new_id).unwrap();
    assert_eq!(new_node.parent_id, Some(1)); // parent is LCA, not root
}

// ── Node merge empty intersection result ──

#[test]
fn test_merge_nodes_intersection_empty() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let data = b"ERROR auth\nWARN memory\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: only ERROR
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: only WARN
    repo.apply_operation_from(0, "warn", Operation::Filter {
        pattern: "WARN".to_string(),
        keep: true,
    })
    .unwrap();

    // Intersection of disjoint sets
    let result = repo.merge_nodes(&[1, 2], "empty", MergeMode::Intersection);
    assert!(result.is_ok());
    let lines = repo.get_current_lines().unwrap();
    assert!(lines.is_empty());
}

// ── Verify compute_state_at works for merge nodes (regression test) ──

#[test]
fn test_compute_state_at_merge_node() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let data = b"ERROR auth\nWARN memory\nERROR disk\nINFO startup\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR → "ERROR auth", "ERROR disk"
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep WARN → "WARN memory"
    repo.apply_operation_from(0, "warn-branch", Operation::Filter {
        pattern: "WARN".to_string(),
        keep: true,
    })
    .unwrap();

    // Merge node 1 and node 2
    let merge_id = repo.merge_nodes(&[1, 2], "merged", MergeMode::Union).unwrap();

    // compute_state_at on the merge node should work (was broken before fix)
    let state = repo.compute_state_at(merge_id).unwrap();
    assert_eq!(state.len(), 3);
    assert!(state.contains(&"ERROR auth".to_string()));
    assert!(state.contains(&"ERROR disk".to_string()));
    assert!(state.contains(&"WARN memory".to_string()));

    // Also test via line_count_at
    let count = repo.line_count_at(merge_id).unwrap();
    assert_eq!(count, 3);

    // After losing the cache (switch away and back), it should still work
    repo.checkout_branch("main").unwrap();
    repo.checkout_branch("merged").unwrap();
    let state2 = repo.compute_state_at(merge_id).unwrap();
    assert_eq!(state2.len(), 3);
}

// ── Verify source nodes are not corrupted after merge ──

#[test]
fn test_merge_nodes_sources_unchanged() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");
    let data = b"line with 200 code\nline with 201 code\nanother 200 line\nother 201 line\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: filter keep 201 → "line with 201 code", "other 201 line"
    repo.apply_operation(Operation::Filter {
        pattern: "201".to_string(),
        keep: true,
    })
    .unwrap();
    let node1_original = repo.compute_state_at(1).unwrap();
    assert_eq!(node1_original.len(), 2);
    for l in &node1_original {
        assert!(l.contains("201"), "node 1 should have 201: {}", l);
    }

    // Node 2: filter keep 200 → "line with 200 code", "another 200 line"
    repo.undo().unwrap();
    repo.apply_operation_from(0, "branch-200", Operation::Filter {
        pattern: "200".to_string(),
        keep: true,
    })
    .unwrap();
    let node2_original = repo.compute_state_at(2).unwrap();
    assert_eq!(node2_original.len(), 2);
    for l in &node2_original {
        assert!(l.contains("200"), "node 2 should have 200: {}", l);
    }

    // Merge node 1 and node 2
    let merge_id = repo.merge_nodes(&[1, 2], "merged", MergeMode::Union).unwrap();
    let merge_lines = repo.compute_state_at(merge_id).unwrap();
    assert_eq!(merge_lines.len(), 4);

    // CRITICAL: after merge, source nodes must retain their original content
    let n1_after = repo.compute_state_at(1).unwrap();
    assert_eq!(n1_after.len(), 2, "node 1 should still have 2 lines after merge");
    for l in &n1_after {
        assert!(l.contains("201"), "BUG: node 1 after merge has non-201: {}", l);
    }

    let n2_after = repo.compute_state_at(2).unwrap();
    assert_eq!(n2_after.len(), 2, "node 2 should still have 2 lines after merge");
    for l in &n2_after {
        assert!(l.contains("200"), "BUG: node 2 after merge has non-200: {}", l);
    }

    // Source nodes should have different content
    assert_ne!(n1_after, n2_after, "BUG: nodes 1 and 2 have same content after merge");
}

// ── Node merge preserves LCA line ordering ──

#[test]
fn test_merge_nodes_preserves_lca_ordering() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    // Interleaved data matching the user's example:
    // LCA (root): 01, 02, 03, 01, 01, 02, 02, 03, 03
    let data = b"01\n02\n03\n01\n01\n02\n02\n03\n03\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: filter keep lines matching "01" → "01", "01", "01"
    repo.apply_operation(Operation::Filter {
        pattern: "01".to_string(),
        keep: true,
    })
    .unwrap();
    let node1_lines = repo.compute_state_at(1).unwrap();
    assert_eq!(node1_lines, vec!["01", "01", "01"]);

    repo.undo().unwrap();

    // Node 2: filter keep lines matching "02" → "02", "02", "02"
    repo.apply_operation_from(0, "branch-02", Operation::Filter {
        pattern: "02".to_string(),
        keep: true,
    })
    .unwrap();
    let node2_lines = repo.compute_state_at(2).unwrap();
    assert_eq!(node2_lines, vec!["02", "02", "02"]);

    // Union merge: result should preserve LCA ordering
    let merge_id = repo.merge_nodes(&[1, 2], "merged", MergeMode::Union).unwrap();
    let merged = repo.compute_state_at(merge_id).unwrap();

    // Expected: walk LCA, keep lines that are in either source
    // LCA: 01, 02, 03, 01, 01, 02, 02, 03, 03
    // Keep 01s and 02s, skip 03s
    assert_eq!(
        merged,
        vec!["01", "02", "01", "01", "02", "02"],
        "Union merge must preserve LCA ordering, not concatenate sources"
    );
}

// ── Node merge intersection with LCA ordering ──

#[test]
fn test_merge_nodes_intersection_lca_ordering() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    // Data: a, b, c, a, b
    let data = b"a\nb\nc\na\nb\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep a, b → "a", "b", "a", "b"
    repo.apply_operation(Operation::Filter {
        pattern: "[ab]".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep a, c → "a", "c", "a"
    repo.apply_operation_from(0, "branch-ac", Operation::Filter {
        pattern: "[ac]".to_string(),
        keep: true,
    })
    .unwrap();

    // Intersection: only "a" is in both. With multiplicities:
    // S1 has 2 "a"s, S2 has 2 "a"s → intersection has 2 "a"s
    // LCA order: a, b, c, a, b → filter to intersection: a, a
    let merge_id = repo.merge_nodes(&[1, 2], "inter", MergeMode::Intersection).unwrap();
    let merged = repo.compute_state_at(merge_id).unwrap();
    assert_eq!(merged, vec!["a", "a"],
        "Intersection must preserve LCA ordering");
}

// ── Node subtract (set difference) ──

#[test]
fn test_subtract_nodes() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"ERROR auth\nWARN memory\nERROR disk\nINFO startup\nDEBUG trace\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERRORs → ["ERROR auth", "ERROR disk"]
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();

    // Node 2: keep lines with "disk" (from original)
    repo.apply_operation_from(0, "disk-branch", Operation::Filter {
        pattern: "disk".to_string(),
        keep: true,
    })
    .unwrap();

    // Subtract node 2 from node 1: lines in node 1 NOT in node 2
    // node 1: ["ERROR auth", "ERROR disk"]
    // node 2: ["ERROR disk"]
    // result: ["ERROR auth"]
    let new_id = repo.subtract_nodes(1, 2, "diff-branch").unwrap();

    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], "ERROR auth");

    let new_node = repo.history_tree().get_node(new_id).unwrap();
    match new_node.operation.as_ref().unwrap() {
        Operation::Subtract { base, subtrahend } => {
            assert_eq!(*base, 1);
            assert_eq!(*subtrahend, 2);
        }
        _ => panic!("Expected Subtract operation"),
    }
}

// ── Node replay (copy at different position) ──

#[test]
fn test_replay_node_at_different_position() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"ERROR auth\nWARN memory\nERROR disk\nINFO startup\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: filter keep ERROR (on original)
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    // State: ["ERROR auth", "ERROR disk"]

    // Now replay node 1's filter at the root again (re-apply the same filter)
    // Since root state is all lines, same result expected
    let new_id = repo.replay_node_at(1, 0, "replay-branch").unwrap();

    let lines = repo.get_current_lines().unwrap();
    assert_eq!(lines, vec!["ERROR auth", "ERROR disk"]);

    let new_node = repo.history_tree().get_node(new_id).unwrap();
    match new_node.operation.as_ref().unwrap() {
        Operation::Replay { source_node_id } => {
            assert_eq!(*source_node_id, 1);
        }
        _ => panic!("Expected Replay operation"),
    }
}

#[test]
fn test_replay_node_preserves_tag_scope() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"A\nB\nC\nD\nE\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    let scope = lograph::tag::TagScopeRef {
        tag_name: "test-scope".into(),
        ranges: vec![(1, 3)],
    };

    // Create a scoped filter
    repo.apply_operation_scoped(
        Operation::Filter {
            pattern: "[BCD]".to_string(),
            keep: true,
        },
        Some(scope),
    )
    .unwrap();

    // Replay this scoped operation at root
    let new_id = repo.replay_node_at(1, 0, "replay-scoped").unwrap();

    let new_node = repo.history_tree().get_node(new_id).unwrap();
    assert!(new_node.tag_scope.is_some());
    assert_eq!(new_node.tag_scope.as_ref().unwrap().tag_name, "test-scope");
}

// ── Soft delete ──

#[test]
fn test_soft_delete_node_marks_deleted() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"A\nB\nC\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    repo.apply_operation(Operation::Filter {
        pattern: "A".to_string(),
        keep: true,
    })
    .unwrap();

    assert_eq!(repo.history_tree().len(), 2);

    repo.soft_delete_node(1).unwrap();

    let node = repo.history_tree().get_node(1).unwrap();
    assert!(node.deleted);
    // Branch should be moved to parent (root)
    assert_eq!(repo.head_node_id(), 0);
}

#[test]
fn test_soft_delete_root_fails() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"A\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    let result = repo.soft_delete_node(0);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("root"));
}

// ── Soft delete cascades to dependent merge nodes ──

#[test]
fn test_soft_delete_cascades_to_merge_node() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"ERROR auth\nWARN memory\nERROR disk\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep WARN
    repo.apply_operation_from(0, "warn", Operation::Filter {
        pattern: "WARN".to_string(),
        keep: true,
    })
    .unwrap();

    // Merge node 1 and node 2 → creates node 3
    let merge_id = repo.merge_nodes(&[1, 2], "merged", MergeMode::Union).unwrap();
    assert_eq!(merge_id, 3);

    // Delete node 1 → should cascade-delete merge node 3
    let count = repo.soft_delete_node(1).unwrap();
    assert_eq!(count, 2, "should delete node 1 + merge node 3 = 2 nodes");

    let n1 = repo.history_tree().get_node(1).unwrap();
    assert!(n1.deleted);
    let n3 = repo.history_tree().get_node(3).unwrap();
    assert!(n3.deleted, "merge node should be cascade-deleted");
}

// ── Soft delete cascades to descendants of merge node ──

#[test]
fn test_soft_delete_cascades_to_merge_descendants() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"ERROR auth\nWARN memory\nERROR disk\nINFO startup\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep WARN
    repo.apply_operation_from(0, "warn", Operation::Filter {
        pattern: "WARN".to_string(),
        keep: true,
    })
    .unwrap();

    // Merge node 1 and node 2 → creates node 3
    let merge_id = repo.merge_nodes(&[1, 2], "merged", MergeMode::Union).unwrap();

    // Apply a filter on top of the merge → node 4 (descendant of merge)
    repo.apply_operation(Operation::Filter {
        pattern: "auth".to_string(),
        keep: true,
    })
    .unwrap();

    // Delete node 1 → should cascade: merge node 3 + its child node 4
    let count = repo.soft_delete_node(1).unwrap();
    assert_eq!(count, 3, "should delete node 1 + merge 3 + child 4 = 3 nodes");

    assert!(repo.history_tree().get_node(1).unwrap().deleted);
    assert!(repo.history_tree().get_node(merge_id).unwrap().deleted);
    assert!(repo.history_tree().get_node(4).unwrap().deleted);
}

// ── Soft delete cascades to subtract nodes ──

#[test]
fn test_soft_delete_cascades_to_subtract_node() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"ERROR auth\nWARN memory\nERROR disk\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep disk
    repo.apply_operation_from(0, "disk", Operation::Filter {
        pattern: "disk".to_string(),
        keep: true,
    })
    .unwrap();

    // Subtract node 2 from node 1 → creates node 3
    let sub_id = repo.subtract_nodes(1, 2, "diff").unwrap();
    assert_eq!(sub_id, 3);

    // Delete node 1 (the base of Subtract) → cascade-delete node 3
    let count = repo.soft_delete_node(1).unwrap();
    assert_eq!(count, 2);

    assert!(repo.history_tree().get_node(3).unwrap().deleted);
}

// ── Soft delete cascades to replay nodes ──

#[test]
fn test_soft_delete_cascades_to_replay_node() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"ERROR auth\nWARN memory\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();

    // Replay node 1 at root → creates node 2
    let replay_id = repo.replay_node_at(1, 0, "replay").unwrap();
    assert_eq!(replay_id, 2);

    // Delete node 1 → cascade-delete replay node 2
    let count = repo.soft_delete_node(1).unwrap();
    assert_eq!(count, 2);

    assert!(repo.history_tree().get_node(2).unwrap().deleted);
}

// ── Soft delete: no cascade for plain filter nodes ──

#[test]
fn test_soft_delete_no_cascade_for_plain_node() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"A\nB\nC\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: filter (no dependents)
    repo.apply_operation(Operation::Filter {
        pattern: "A".to_string(),
        keep: true,
    })
    .unwrap();

    let count = repo.soft_delete_node(1).unwrap();
    assert_eq!(count, 1, "only the node itself should be deleted");
}

// ── Soft delete: multi-level cascade (merge of merge) ──

#[test]
fn test_soft_delete_cascades_multi_level() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"A\nB\nC\nD\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep A
    repo.apply_operation(Operation::Filter {
        pattern: "A".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep B
    repo.apply_operation_from(0, "b", Operation::Filter {
        pattern: "B".to_string(),
        keep: true,
    })
    .unwrap();

    // Merge 1+2 → node 3
    repo.merge_nodes(&[1, 2], "m1", MergeMode::Union).unwrap();

    // Node 4: keep C (from root)
    repo.checkout_branch("main").unwrap();
    repo.apply_operation_from(0, "c", Operation::Filter {
        pattern: "C".to_string(),
        keep: true,
    })
    .unwrap();

    // Merge 3+4 → node 5 (merge of a merge)
    let merge2_id = repo.merge_nodes(&[3, 4], "m2", MergeMode::Union).unwrap();

    // Delete node 1 → cascade: 3 (merge), 5 (merge of merge)
    let count = repo.soft_delete_node(1).unwrap();
    assert_eq!(count, 3, "node 1 + merge 3 + merge 5 = 3");

    assert!(repo.history_tree().get_node(3).unwrap().deleted);
    assert!(repo.history_tree().get_node(merge2_id).unwrap().deleted);
}

// ── Soft delete: deleting subtrahend also cascades ──

#[test]
fn test_soft_delete_cascades_subtrahend() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"ERROR auth\nWARN memory\nERROR disk\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    // Node 1: keep ERROR
    repo.apply_operation(Operation::Filter {
        pattern: "ERROR".to_string(),
        keep: true,
    })
    .unwrap();
    repo.undo().unwrap();

    // Node 2: keep disk
    repo.apply_operation_from(0, "disk", Operation::Filter {
        pattern: "disk".to_string(),
        keep: true,
    })
    .unwrap();

    // Subtract: node 1 minus node 2 → node 3
    let sub_id = repo.subtract_nodes(1, 2, "diff").unwrap();

    // Delete node 2 (the subtrahend) → cascade-delete subtract node 3
    let count = repo.soft_delete_node(2).unwrap();
    assert_eq!(count, 2, "node 2 + subtract node 3");

    assert!(repo.history_tree().get_node(sub_id).unwrap().deleted);
}

// ── History tree integration with tags ──

#[test]
fn test_history_tree_shows_tag_name() {
    let tmp = TempDir::new().unwrap();
    let repo_path = tmp.path().join("repo");

    let data = b"A\nB\nC\n";
    let mut repo = LogRepo::import_from_bytes(&repo_path, data, "test".into()).unwrap();

    let scope = lograph::tag::TagScopeRef {
        tag_name: "important".into(),
        ranges: vec![(0, 1)],
    };

    repo.apply_operation_scoped(
        Operation::Filter {
            pattern: ".".to_string(),
            keep: true,
        },
        Some(scope),
    )
    .unwrap();

    let order = repo.history_tree().topological_order();
    assert_eq!(order.len(), 2);
    // The child node should have the tag name
    assert_eq!(order[1].tag_name, Some("important".to_string()));
}
