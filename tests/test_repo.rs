use std::fs;
use tempfile::TempDir;

use lga_core::operator::Operation;
use lga_core::repo::LogRepo;

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
    use lga_core::engine::{CollectResult, Collector};
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
