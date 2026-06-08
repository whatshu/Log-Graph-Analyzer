mod metadata;
mod storage;
mod workspace;

pub use metadata::RepoMetadata;
pub use storage::ChunkStorage;
pub use workspace::{Workspace, DEFAULT_REPO_NAME};

use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::{collector, ChunkedProcessor, CollectResult, Collector, LineStream};
use crate::error::{LogAnalyzerError, Result};
use crate::history::HistoryTree;
use crate::index::{IndexBuilder, LineIndex};
use crate::operator::{InverseData, Operation, OperationRecord};
use crate::tag::{self, TagScopeRef};

/// A log repository that stores compressed log data with operation history.
///
/// Directory layout:
/// ```text
/// <repo_path>/
/// ├── meta.json           # Repository metadata
/// ├── index.json          # Line index
/// ├── chunks/             # Compressed data chunks
/// │   ├── 000000.zst
/// │   ├── 000001.zst
/// │   └── ...
/// ├── operations.json     # Operation history tree
/// └── snapshots/          # Materialized snapshots after operations
///     └── ...
/// ```
pub struct LogRepo {
    path: PathBuf,
    pub metadata: RepoMetadata,
    pub index: LineIndex,
    storage: ChunkStorage,
    /// Tree-structured operation history (git-like branching).
    history: HistoryTree,
    /// Cached current state lines (after all operations applied along current branch).
    /// None means we need to recompute.
    current_lines: Option<Vec<String>>,
}

impl LogRepo {
    /// Import a text file into a new log repository.
    pub fn import(repo_path: &Path, source_file: &Path) -> Result<Self> {
        if repo_path.exists() {
            return Err(LogAnalyzerError::Repo(format!(
                "Repository already exists: {}",
                repo_path.display()
            )));
        }

        // Read source file
        let data = fs::read(source_file)?;

        Self::import_from_bytes(repo_path, &data, source_file.to_string_lossy().to_string())
    }

    /// Import raw bytes into a new log repository.
    pub fn import_from_bytes(
        repo_path: &Path,
        data: &[u8],
        source_name: String,
    ) -> Result<Self> {
        // Create directory structure
        fs::create_dir_all(repo_path)?;
        fs::create_dir_all(repo_path.join("chunks"))?;
        fs::create_dir_all(repo_path.join("snapshots"))?;

        // Build line index and chunk data
        let builder = IndexBuilder::new();
        let (index, chunks_data) = builder.build(data);

        // Create chunk storage and write compressed chunks
        let storage = ChunkStorage::new(repo_path.join("chunks"));
        storage.write_chunks(&chunks_data)?;

        // Create metadata
        let metadata = RepoMetadata::new(
            source_name,
            data.len() as u64,
            index.total_lines,
        );

        // Save index and metadata
        let index_json = serde_json::to_string_pretty(&index)?;
        fs::write(repo_path.join("index.json"), index_json)?;

        let meta_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(repo_path.join("meta.json"), meta_json)?;

        // Create empty history tree
        let history = HistoryTree::new();
        fs::write(
            repo_path.join("operations.json"),
            history.to_json().unwrap_or_default(),
        )?;

        Ok(Self {
            path: repo_path.to_path_buf(),
            metadata,
            index,
            storage,
            history,
            current_lines: None,
        })
    }

    /// Open an existing log repository.
    /// Automatically migrates old flat format to tree format.
    pub fn open(repo_path: &Path) -> Result<Self> {
        if !repo_path.exists() {
            return Err(LogAnalyzerError::Repo(format!(
                "Repository not found: {}",
                repo_path.display()
            )));
        }

        let metadata: RepoMetadata =
            serde_json::from_str(&fs::read_to_string(repo_path.join("meta.json"))?)?;

        let index: LineIndex =
            serde_json::from_str(&fs::read_to_string(repo_path.join("index.json"))?)?;

        let storage = ChunkStorage::new(repo_path.join("chunks"));

        // Load history — auto-migrates old flat format to tree
        let ops_path = repo_path.join("operations.json");
        let history = if ops_path.exists() {
            let json = fs::read_to_string(&ops_path)?;
            HistoryTree::from_json(&json).unwrap_or_else(|_| {
                // If parsing fails completely, start fresh
                let mut tree = HistoryTree::new();
                // Update root timestamp to match metadata
                if let Some(root) = tree.nodes.get_mut(0) {
                    root.applied_at = metadata.created_at;
                }
                tree
            })
        } else {
            let mut tree = HistoryTree::new();
            tree.nodes[0].applied_at = metadata.created_at;
            tree
        };

        Ok(Self {
            path: repo_path.to_path_buf(),
            metadata,
            index,
            storage,
            history,
            current_lines: None,
        })
    }

    /// Append data from a file into this repository.
    pub fn append_file(&mut self, source_file: &Path) -> Result<usize> {
        let data = fs::read(source_file)?;
        self.append_bytes(&data)
    }

    /// Append raw bytes into this repository.
    /// Returns the number of new lines appended.
    pub fn append_bytes(&mut self, data: &[u8]) -> Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }

        let existing_chunks = self.index.chunks.len() as u32;
        let existing_lines = self.index.total_lines;

        // Build index for new data, using the same lines_per_chunk
        let builder = IndexBuilder::new().with_lines_per_chunk(self.index.lines_per_chunk);
        let (mut new_index, new_chunks_data) = builder.build(data);

        if new_index.total_lines == 0 {
            return Ok(0);
        }

        // Adjust new chunk IDs and line_start offsets
        for chunk in &mut new_index.chunks {
            chunk.id += existing_chunks;
            chunk.line_start += existing_lines;
        }

        // Write new compressed chunks (IDs continue from existing)
        self.storage.write_chunks_at(&new_chunks_data, existing_chunks)?;

        // Extend the index
        self.index.total_lines += new_index.total_lines;
        self.index.chunks.extend(new_index.chunks);

        // Update metadata
        self.metadata.original_size += data.len() as u64;
        self.metadata.original_line_count = self.index.total_lines;

        // Persist updated index and metadata
        self.save_index()?;
        self.save_metadata()?;

        // Invalidate current lines cache (operations need to re-apply over full data)
        self.current_lines = None;

        Ok(new_index.total_lines)
    }

    /// Clone this repository to a new path.
    pub fn clone_to(&self, dest_path: &Path) -> Result<Self> {
        if dest_path.exists() {
            return Err(LogAnalyzerError::Repo(format!(
                "Destination already exists: {}",
                dest_path.display()
            )));
        }
        // Copy the entire directory tree
        copy_dir_all(&self.path, dest_path)?;
        Self::open(dest_path)
    }

    /// Read a single line from the original (unmodified) data.
    pub fn read_original_line(&self, line_num: usize) -> Result<String> {
        if line_num >= self.index.total_lines {
            return Err(LogAnalyzerError::LineOutOfRange(
                line_num,
                self.index.total_lines,
            ));
        }

        let (chunk_idx, line_in_chunk) = self
            .index
            .locate_line(line_num)
            .ok_or(LogAnalyzerError::LineOutOfRange(
                line_num,
                self.index.total_lines,
            ))?;

        let chunk_data = self.storage.read_chunk(chunk_idx as u32)?;
        let (start, end) = self.index.line_range_in_chunk(chunk_idx, line_in_chunk);
        let actual_end = end.min(chunk_data.len());

        let line = String::from_utf8_lossy(&chunk_data[start..actual_end]);
        // Strip trailing newline
        Ok(line.trim_end_matches('\n').to_string())
    }

    /// Read a range of lines from original data.
    pub fn read_original_lines(&self, start: usize, count: usize) -> Result<Vec<String>> {
        let end = (start + count).min(self.index.total_lines);
        let mut lines = Vec::with_capacity(end - start);

        // Group by chunk for efficiency
        let mut current_chunk_idx: Option<usize> = None;
        let mut current_chunk_data: Vec<u8> = Vec::new();

        for line_num in start..end {
            let (chunk_idx, line_in_chunk) = self
                .index
                .locate_line(line_num)
                .ok_or(LogAnalyzerError::LineOutOfRange(
                    line_num,
                    self.index.total_lines,
                ))?;

            // Load chunk if needed
            if current_chunk_idx != Some(chunk_idx) {
                current_chunk_data = self.storage.read_chunk(chunk_idx as u32)?;
                current_chunk_idx = Some(chunk_idx);
            }

            let (start_byte, end_byte) =
                self.index.line_range_in_chunk(chunk_idx, line_in_chunk);
            let actual_end = end_byte.min(current_chunk_data.len());

            let line = String::from_utf8_lossy(&current_chunk_data[start_byte..actual_end]);
            lines.push(line.trim_end_matches('\n').to_string());
        }

        Ok(lines)
    }

    /// Read all original lines. Use carefully for large files.
    pub fn read_all_original_lines(&self) -> Result<Vec<String>> {
        self.read_original_lines(0, self.index.total_lines)
    }

    /// Get the current state of all lines (after applying all operations on current branch).
    pub fn get_current_lines(&mut self) -> Result<Vec<String>> {
        if let Some(ref lines) = self.current_lines {
            return Ok(lines.clone());
        }

        let head_id = self.history.head();
        let lines = self.compute_state_at(head_id)?;

        self.current_lines = Some(lines.clone());
        Ok(lines)
    }

    /// Get the number of lines in current state.
    pub fn current_line_count(&mut self) -> Result<usize> {
        let lines = self.get_current_lines()?;
        Ok(lines.len())
    }

    /// Compute the line state at a given node in the history tree.
    /// Walks from root to the target node, applying operations along the path.
    pub fn compute_state_at(&self, node_id: usize) -> Result<Vec<String>> {
        let path = self.history.path_to(node_id);
        if path.is_empty() {
            return Err(LogAnalyzerError::Repo(format!(
                "Node {} not found in history tree",
                node_id
            )));
        }

        let mut lines = self.read_all_original_lines()?;

        // Apply operations along the path (skip root at index 0)
        for &nid in &path[1..] {
            if let Some(node) = self.history.get_node(nid) {
                if let Some(ref op) = node.operation {
                    lines = op.apply(lines)?;
                }
            }
        }

        Ok(lines)
    }

    /// Compute the line count at a given node (without full materialization if possible).
    pub fn line_count_at(&self, node_id: usize) -> Result<usize> {
        if node_id == 0 {
            return Ok(self.index.total_lines);
        }
        let lines = self.compute_state_at(node_id)?;
        Ok(lines.len())
    }

    // ── Branch and history operations ──

    /// Switch to a named branch. The branch must exist.
    pub fn checkout_branch(&mut self, name: &str) -> Result<()> {
        if !self.history.checkout_branch(name) {
            return Err(LogAnalyzerError::Repo(format!(
                "Branch '{}' does not exist",
                name
            )));
        }
        self.current_lines = None;
        self.save_history()?;
        Ok(())
    }

    /// Checkout (detached) to a specific node for viewing.
    /// This does NOT change any branch HEAD — it's for read-only viewing.
    /// Returns the lines at that node.
    pub fn view_node(&self, node_id: usize) -> Result<Vec<String>> {
        self.compute_state_at(node_id)
    }

    /// Create a new branch at a given node and optionally switch to it.
    /// Returns false if the branch name already exists.
    pub fn create_branch(&mut self, name: &str, at_node_id: usize) -> Result<bool> {
        let created = self.history.create_branch(name, at_node_id);
        if created {
            self.save_history()?;
        }
        Ok(created)
    }

    /// Delete a branch (cannot delete "main" or current branch).
    pub fn delete_branch(&mut self, name: &str) -> Result<bool> {
        let deleted = self.history.delete_branch(name);
        if deleted {
            self.save_history()?;
        }
        Ok(deleted)
    }

    /// Create a new branch from a given node and switch to it.
    /// This is the "branch off from historical node" operation.
    pub fn branch_from(
        &mut self,
        branch_name: &str,
        from_node_id: usize,
    ) -> Result<()> {
        if !self.history.create_branch(branch_name, from_node_id) {
            return Err(LogAnalyzerError::Repo(format!(
                "Branch '{}' already exists",
                branch_name
            )));
        }
        self.history.checkout_branch(branch_name);
        self.current_lines = None;
        self.save_history()?;
        Ok(())
    }

    /// Read lines from the current state (after operations).
    pub fn read_current_lines(&mut self, start: usize, count: usize) -> Result<Vec<String>> {
        let lines = self.get_current_lines()?;
        if start >= lines.len() {
            return Err(LogAnalyzerError::LineOutOfRange(start, lines.len()));
        }
        let end = (start + count).min(lines.len());
        Ok(lines[start..end].to_vec())
    }

    /// Apply an operation to the current state (current branch HEAD).
    pub fn apply_operation(&mut self, operation: Operation) -> Result<()> {
        self.apply_operation_scoped(operation, None)
    }

    /// Apply an operation with an optional tag scope.
    /// When `scope` is set, only lines within the scope ranges are affected;
    /// lines outside the scope pass through unchanged.
    pub fn apply_operation_scoped(
        &mut self,
        operation: Operation,
        scope: Option<TagScopeRef>,
    ) -> Result<()> {
        let lines = self.get_current_lines()?;

        let (new_lines, inverse) = if let Some(ref s) = scope {
            // Extract scoped subset, apply op, merge back
            let scoped = tag::filter_lines_by_ranges(&lines, &s.ranges);
            let (scoped_new, inv) = operation.apply_with_inverse(scoped)?;

            // Merge scoped result back into the full line set
            // Build a map: for each range, which lines replace the originals
            let new_full = merge_scoped_result(&lines, &s.ranges, &scoped_new);
            (new_full, inv)
        } else {
            operation.apply_with_inverse(lines)?
        };

        let head = self.history.head();
        let new_node_id = self.history.add_child_with_scope(
            head,
            operation,
            inverse,
            scope,
        );

        self.history
            .advance_branch(&self.history.current_branch.clone(), new_node_id);
        self.current_lines = Some(new_lines);

        self.save_history()?;
        Ok(())
    }

    /// Apply an operation from a specific node (for branching off).
    /// Creates a child of the given node, advances/creates the branch.
    pub fn apply_operation_from(
        &mut self,
        from_node_id: usize,
        branch_name: &str,
        operation: Operation,
    ) -> Result<()> {
        self.apply_operation_from_scoped(from_node_id, branch_name, operation, None)
    }

    /// Apply an operation from a specific node with optional tag scope.
    pub fn apply_operation_from_scoped(
        &mut self,
        from_node_id: usize,
        branch_name: &str,
        operation: Operation,
        scope: Option<TagScopeRef>,
    ) -> Result<()> {
        let lines = self.compute_state_at(from_node_id)?;

        let (new_lines, inverse) = if let Some(ref s) = scope {
            let scoped = tag::filter_lines_by_ranges(&lines, &s.ranges);
            let (scoped_new, inv) = operation.apply_with_inverse(scoped)?;
            let new_full = merge_scoped_result(&lines, &s.ranges, &scoped_new);
            (new_full, inv)
        } else {
            operation.apply_with_inverse(lines)?
        };

        let new_node_id =
            self.history
                .add_child_with_scope(from_node_id, operation, inverse, scope);

        if !self.history.branches.contains_key(branch_name) {
            self.history.create_branch(branch_name, from_node_id);
        }
        self.history.advance_branch(branch_name, new_node_id);
        self.history.checkout_branch(branch_name);

        self.current_lines = Some(new_lines);
        self.save_history()?;
        Ok(())
    }

    /// Merge multiple source nodes: create a new node whose state is the
    /// UNION of all line sets at each source node.
    ///
    /// Lines are compared by exact string match. Returns the new node ID.
    pub fn merge_nodes(
        &mut self,
        sources: &[usize],
        branch_name: &str,
    ) -> Result<usize> {
        if sources.is_empty() {
            return Err(LogAnalyzerError::Operator(
                "Need at least one source node to merge".into(),
            ));
        }

        let mut line_sets: Vec<Vec<String>> = Vec::new();
        for &sid in sources {
            line_sets.push(self.compute_state_at(sid)?);
        }

        let merged_lines = tag::union_line_sets(&line_sets);

        let operation = Operation::Merge {
            sources: sources.to_vec(),
        };
        let inverse = InverseData::MergeInverse {
            source_line_sets: line_sets,
        };

        // Attach to the first source (arbitrary parent choice)
        let parent_id = sources[0];
        let new_node_id =
            self.history
                .add_child(parent_id, operation, inverse);

        if !self.history.branches.contains_key(branch_name) {
            self.history.create_branch(branch_name, parent_id);
        }
        self.history.advance_branch(branch_name, new_node_id);
        self.history.checkout_branch(branch_name);

        self.current_lines = Some(merged_lines);
        self.save_history()?;
        Ok(new_node_id)
    }

    /// Subtract one node's line set from another: create a new node with
    /// lines that exist in `base` but NOT in `subtrahend`.
    ///
    /// Returns the new node ID.
    pub fn subtract_nodes(
        &mut self,
        base: usize,
        subtrahend: usize,
        branch_name: &str,
    ) -> Result<usize> {
        let base_lines = self.compute_state_at(base)?;
        let subtrahend_lines = self.compute_state_at(subtrahend)?;

        let diff_lines = tag::subtract_line_sets(&base_lines, &subtrahend_lines);
        let removed: Vec<(usize, String)> = subtrahend_lines
            .iter()
            .enumerate()
            .map(|(i, s)| (i, s.clone()))
            .collect();

        let operation = Operation::Subtract { base, subtrahend };
        let inverse = InverseData::SubtractInverse { removed };

        let new_node_id = self.history.add_child(base, operation, inverse);

        if !self.history.branches.contains_key(branch_name) {
            self.history.create_branch(branch_name, base);
        }
        self.history.advance_branch(branch_name, new_node_id);
        self.history.checkout_branch(branch_name);

        self.current_lines = Some(diff_lines);
        self.save_history()?;
        Ok(new_node_id)
    }

    /// Replay (copy) a source node's operation at a different position in
    /// the tree. The source node must have a replayable operation (Filter,
    /// Replace, DeleteLines, InsertLines, or ModifyLine).
    ///
    /// Returns the new node ID.
    pub fn replay_node_at(
        &mut self,
        source_node_id: usize,
        target_parent_id: usize,
        branch_name: &str,
    ) -> Result<usize> {
        let src_node = self
            .history
            .get_node(source_node_id)
            .ok_or_else(|| {
                LogAnalyzerError::Repo(format!(
                    "Source node {} not found",
                    source_node_id
                ))
            })?;

        let src_op = src_node
            .operation
            .as_ref()
            .ok_or_else(|| {
                LogAnalyzerError::Operator(
                    "Source node has no operation to replay".into(),
                )
            })?;

        // Validate that the source operation is replayable
        match src_op {
            Operation::Merge { .. } | Operation::Subtract { .. } | Operation::Replay { .. } => {
                return Err(LogAnalyzerError::Operator(
                    "Cannot replay Merge/Subtract/Replay operations".into(),
                ));
            }
            _ => {} // OK — Filter, Replace, DeleteLines, InsertLines, ModifyLine
        }

        // Compute state at target parent
        let target_lines = self.compute_state_at(target_parent_id)?;

        // Apply source operation to target state
        // We need to handle tag scope from the source node
        let (new_lines, inverse) = if let Some(ref scope) = src_node.tag_scope {
            let scoped = tag::filter_lines_by_ranges(&target_lines, &scope.ranges);
            let (scoped_new, inv) = src_op.apply_with_inverse(scoped)?;
            let new_full = merge_scoped_result(&target_lines, &scope.ranges, &scoped_new);
            (new_full, inv)
        } else {
            src_op.apply_with_inverse(target_lines)?
        };

        let operation = Operation::Replay {
            source_node_id,
        };
        let replay_inverse = InverseData::ReplayInverse {
            inner: Box::new(inverse),
        };

        // Preserve tag scope from source node
        let tag_scope = src_node.tag_scope.clone();

        let new_node_id = self.history.add_child_with_scope(
            target_parent_id,
            operation,
            replay_inverse,
            tag_scope,
        );

        if !self.history.branches.contains_key(branch_name) {
            self.history.create_branch(branch_name, target_parent_id);
        }
        self.history.advance_branch(branch_name, new_node_id);
        self.history.checkout_branch(branch_name);

        self.current_lines = Some(new_lines);
        self.save_history()?;
        Ok(new_node_id)
    }

    /// Soft-delete a history node. The node is marked deleted and its
    /// children are reparented to its parent. Branch pointers are updated.
    ///
    /// The root node cannot be deleted.
    pub fn soft_delete_node(&mut self, node_id: usize) -> Result<()> {
        self.history
            .soft_delete(node_id)
            .map_err(|msg| LogAnalyzerError::Repo(msg))?;

        // Invalidate cache since tree structure changed
        self.current_lines = None;
        self.save_history()?;
        Ok(())
    }

    /// Filter lines to only those within the given tag scope ranges.
    pub fn filter_by_scope(
        &self,
        lines: &[String],
        scope: &TagScopeRef,
    ) -> Vec<String> {
        tag::filter_lines_by_ranges(lines, &scope.ranges)
    }

    /// Undo the last operation on the current branch.
    /// Non-destructive: moves branch HEAD back to parent without deleting nodes.
    pub fn undo(&mut self) -> Result<Operation> {
        let head = self.history.head();
        if head == 0 {
            return Err(LogAnalyzerError::NoOperationsToUndo);
        }

        let undone_op = self
            .history
            .undo()
            .cloned()
            .ok_or(LogAnalyzerError::NoOperationsToUndo)?;

        // Invalidate cache
        self.current_lines = None;

        self.save_history()?;
        Ok(undone_op)
    }

    /// Get operation history as a list of records for backward compatibility
    /// (used by Python bindings and TUI building the display).
    pub fn history_records(&self) -> Vec<OperationRecord> {
        let mut records = Vec::new();
        for node in &self.history.nodes {
            if let (Some(op), Some(inv)) = (&node.operation, &node.inverse) {
                records.push(OperationRecord {
                    id: node.id,
                    operation: op.clone(),
                    inverse: inv.clone(),
                    applied_at: node.applied_at,
                });
            }
        }
        records
    }

    /// Get a reference to the history tree.
    pub fn history_tree(&self) -> &HistoryTree {
        &self.history
    }

    /// Get the current branch name.
    pub fn current_branch(&self) -> &str {
        &self.history.current_branch
    }

    /// List all branch names.
    pub fn branch_names(&self) -> Vec<&str> {
        self.history.branch_names()
    }

    /// Get the HEAD node ID of the current branch.
    pub fn head_node_id(&self) -> usize {
        self.history.head()
    }

    /// Get the HEAD node ID of a specific branch.
    pub fn branch_head_node_id(&self, name: &str) -> Option<usize> {
        self.history.branch_head(name)
    }

    /// Export current state to a file.
    pub fn export(&mut self, dest: &Path) -> Result<()> {
        let lines = self.get_current_lines()?;
        let content = lines.join("\n");
        fs::write(dest, content)?;
        Ok(())
    }

    /// Get repository path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get total original line count.
    pub fn original_line_count(&self) -> usize {
        self.index.total_lines
    }

    /// Create a streaming line reader for chunk-by-chunk processing.
    /// Memory usage is O(chunk_size) instead of O(total_lines).
    pub fn line_stream(&self) -> LineStream<'_> {
        LineStream::new(&self.storage, &self.index)
    }

    /// Create a chunked processor for streaming operations on original data.
    /// Use this for large files where loading all lines is impractical.
    pub fn processor(&self) -> ChunkedProcessor<'_> {
        ChunkedProcessor::new(&self.storage, &self.index)
    }

    /// Get a reference to the storage.
    pub fn storage(&self) -> &ChunkStorage {
        &self.storage
    }

    /// Run a collector on the **current** state (original + operations).
    /// Read-only: the repository is not modified.
    ///
    /// If no operations have been applied, the collector runs directly on
    /// the compressed chunks in parallel (memory-efficient).
    /// Otherwise it runs on the materialized current lines.
    pub fn collect(&mut self, c: &Collector) -> Result<CollectResult> {
        if self.history.is_empty() {
            // Fast path: stream over original chunks (O(chunk_size) memory)
            collector::execute(c, &self.storage, &self.index)
        } else {
            // Operations applied — must use the materialized view
            let lines = self.get_current_lines()?;
            collector::execute_on_lines(c, &lines)
        }
    }

    /// Run a collector on the **original** (un-modified) data only.
    /// Always uses the streaming chunk path regardless of operations.
    pub fn collect_original(&self, c: &Collector) -> Result<CollectResult> {
        collector::execute(c, &self.storage, &self.index)
    }

    fn save_history(&self) -> Result<()> {
        let json = self
            .history
            .to_json()
            .unwrap_or_else(|_| "{}".to_string());
        fs::write(self.path.join("operations.json"), json)?;
        Ok(())
    }

    fn save_index(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.index)?;
        fs::write(self.path.join("index.json"), json)?;
        Ok(())
    }

    fn save_metadata(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.metadata)?;
        fs::write(self.path.join("meta.json"), json)?;
        Ok(())
    }
}

/// Merge scoped operation results back into the full line set.
///
/// Takes the original lines, the ranges that were operated on, and the
/// resulting scoped lines, and produces the full merged result.
/// Lines outside the scoped ranges pass through unchanged.  Within each
/// range, the scoped result lines replace the original range lines (the
/// count may differ if lines were filtered out).
fn merge_scoped_result(
    original: &[String],
    ranges: &[(usize, usize)],
    scoped_new: &[String],
) -> Vec<String> {
    // Build a set of all line indices that are within any range.
    use std::collections::HashSet;
    let in_scope: HashSet<usize> = ranges
        .iter()
        .flat_map(|&(s, e)| {
            let end = (e + 1).min(original.len());
            s.min(original.len())..end
        })
        .collect();

    let mut result = Vec::with_capacity(
        original.len() - in_scope.len() + scoped_new.len(),
    );
    let mut scoped_idx = 0usize;

    // Walk through original lines. If a line is in scope, emit the
    // next scoped result line (if any). Otherwise emit the original.
    for (i, line) in original.iter().enumerate() {
        if in_scope.contains(&i) {
            if scoped_idx < scoped_new.len() {
                result.push(scoped_new[scoped_idx].clone());
                scoped_idx += 1;
            }
            // If we've consumed all scoped result lines, skip remaining
            // in-scope originals (they were filtered out or replaced).
        } else {
            result.push(line.clone());
        }
    }

    result
}

pub(crate) fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}
