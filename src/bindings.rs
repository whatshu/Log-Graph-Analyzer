use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::PathBuf;

use crate::engine::{CollectResult, Collector};
use crate::operator::Operation;
use crate::repo::{LogRepo, Workspace};

/// Python wrapper for LogRepo.
#[pyclass(name = "LogRepo")]
pub struct PyLogRepo {
    inner: LogRepo,
}

#[pymethods]
impl PyLogRepo {
    /// Import a text file into a new log repository.
    #[staticmethod]
    fn import_file(repo_path: &str, source_file: &str) -> PyResult<Self> {
        let repo = LogRepo::import(&PathBuf::from(repo_path), &PathBuf::from(source_file))?;
        Ok(Self { inner: repo })
    }

    /// Import raw text into a new log repository.
    #[staticmethod]
    fn import_text(repo_path: &str, text: &str, source_name: &str) -> PyResult<Self> {
        let repo =
            LogRepo::import_from_bytes(&PathBuf::from(repo_path), text.as_bytes(), source_name.to_string())?;
        Ok(Self { inner: repo })
    }

    /// Open an existing log repository.
    #[staticmethod]
    fn open(repo_path: &str) -> PyResult<Self> {
        let repo = LogRepo::open(&PathBuf::from(repo_path))?;
        Ok(Self { inner: repo })
    }

    /// Clone this repository to a new path.
    fn clone_to(&self, dest_path: &str) -> PyResult<PyLogRepo> {
        let new_repo = self.inner.clone_to(&PathBuf::from(dest_path))?;
        Ok(PyLogRepo { inner: new_repo })
    }

    /// Append a text file into this repository. Returns number of new lines.
    fn append_file(&mut self, source_file: &str) -> PyResult<usize> {
        Ok(self.inner.append_file(&PathBuf::from(source_file))?)
    }

    /// Append raw text into this repository. Returns number of new lines.
    fn append_text(&mut self, text: &str) -> PyResult<usize> {
        Ok(self.inner.append_bytes(text.as_bytes())?)
    }

    /// Get repository metadata.
    fn metadata(&self) -> PyResult<PyRepoMetadata> {
        Ok(PyRepoMetadata {
            inner: self.inner.metadata.clone(),
        })
    }

    /// Get total original line count.
    fn original_line_count(&self) -> usize {
        self.inner.original_line_count()
    }

    /// Get current line count (after operations).
    fn current_line_count(&mut self) -> PyResult<usize> {
        Ok(self.inner.current_line_count()?)
    }

    /// Read lines from the current state.
    fn read_lines(&mut self, start: usize, count: usize) -> PyResult<Vec<String>> {
        Ok(self.inner.read_current_lines(start, count)?)
    }

    /// Read all current lines.
    fn read_all_lines(&mut self) -> PyResult<Vec<String>> {
        Ok(self.inner.get_current_lines()?)
    }

    /// Read a single line from the current state.
    fn read_line(&mut self, line_num: usize) -> PyResult<String> {
        let lines = self.inner.read_current_lines(line_num, 1)?;
        Ok(lines.into_iter().next().unwrap_or_default())
    }

    /// Apply a filter operation. `keep=True` keeps matching lines, `keep=False` removes them.
    fn filter(&mut self, pattern: &str, keep: bool) -> PyResult<()> {
        let op = Operation::Filter {
            pattern: pattern.to_string(),
            keep,
        };
        self.inner.apply_operation(op)?;
        Ok(())
    }

    /// Apply a regex replace operation.
    fn replace(&mut self, pattern: &str, replacement: &str) -> PyResult<()> {
        let op = Operation::Replace {
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
        };
        self.inner.apply_operation(op)?;
        Ok(())
    }

    /// Delete specific lines by their indices (0-based).
    fn delete_lines(&mut self, indices: Vec<usize>) -> PyResult<()> {
        let op = Operation::DeleteLines {
            line_indices: indices,
        };
        self.inner.apply_operation(op)?;
        Ok(())
    }

    /// Insert lines after the specified position (0 = insert at beginning).
    fn insert_lines(&mut self, after_line: usize, content: Vec<String>) -> PyResult<()> {
        let op = Operation::InsertLines {
            after_line,
            content,
        };
        self.inner.apply_operation(op)?;
        Ok(())
    }

    /// Modify a single line by index.
    fn modify_line(&mut self, line_index: usize, new_content: &str) -> PyResult<()> {
        let op = Operation::ModifyLine {
            line_index,
            new_content: new_content.to_string(),
        };
        self.inner.apply_operation(op)?;
        Ok(())
    }

    /// Undo the last operation. Returns a description of the undone operation.
    fn undo(&mut self) -> PyResult<String> {
        let op = self.inner.undo()?;
        Ok(op.describe())
    }

    /// Get operation history as a list of OperationRecord.
    fn history(&self) -> Vec<PyOperationRecord> {
        self.inner
            .history_records()
            .iter()
            .map(|r| PyOperationRecord {
                id: r.id,
                description: r.operation.describe(),
                applied_at: r.applied_at.to_rfc3339(),
            })
            .collect()
    }

    /// Export current state to a file.
    fn export(&mut self, dest_path: &str) -> PyResult<()> {
        self.inner.export(&PathBuf::from(dest_path))?;
        Ok(())
    }

    /// Get the repository path.
    fn path(&self) -> String {
        self.inner.path().to_string_lossy().to_string()
    }

    // ── Branch management ──

    /// View (non-destructive checkout) a specific history node.
    /// Returns lines at that node without changing any branch HEAD.
    fn view_node(&self, node_id: usize) -> PyResult<Vec<String>> {
        Ok(self.inner.view_node(node_id)?)
    }

    /// Switch to a named branch.
    fn checkout_branch(&mut self, name: &str) -> PyResult<()> {
        Ok(self.inner.checkout_branch(name)?)
    }

    /// Create a new branch at a given node. Returns true if created.
    fn create_branch(&mut self, name: &str, at_node_id: usize) -> PyResult<bool> {
        Ok(self.inner.create_branch(name, at_node_id)?)
    }

    /// Delete a named branch. Cannot delete "main" or the current branch.
    fn delete_branch(&mut self, name: &str) -> PyResult<bool> {
        Ok(self.inner.delete_branch(name)?)
    }

    /// Create a branch from a node and switch to it.
    fn branch_from(&mut self, branch_name: &str, from_node_id: usize) -> PyResult<()> {
        Ok(self.inner.branch_from(branch_name, from_node_id)?)
    }

    /// List all branch names.
    fn branch_names(&self) -> Vec<String> {
        self.inner.branch_names().into_iter().map(|s| s.to_string()).collect()
    }

    /// Get the current branch name.
    fn current_branch_name(&self) -> String {
        self.inner.current_branch().to_string()
    }

    /// Get the HEAD node ID of current branch.
    fn head_node_id(&self) -> usize {
        self.inner.head_node_id()
    }

    /// Get the HEAD node ID for a specific branch.
    fn branch_head_node_id(&self, name: &str) -> Option<usize> {
        self.inner.branch_head_node_id(name)
    }

    /// Apply a filter operation from a specific history node (branch off).
    /// `keep=True` keeps matching lines.
    fn filter_from(
        &mut self,
        from_node_id: usize,
        branch_name: &str,
        pattern: &str,
        keep: bool,
    ) -> PyResult<()> {
        let op = Operation::Filter {
            pattern: pattern.to_string(),
            keep,
        };
        self.inner
            .apply_operation_from(from_node_id, branch_name, op)?;
        Ok(())
    }

    /// Apply a replace operation from a specific history node (branch off).
    fn replace_from(
        &mut self,
        from_node_id: usize,
        branch_name: &str,
        pattern: &str,
        replacement: &str,
    ) -> PyResult<()> {
        let op = Operation::Replace {
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
        };
        self.inner
            .apply_operation_from(from_node_id, branch_name, op)?;
        Ok(())
    }

    /// Get the total number of nodes in the history tree.
    fn history_tree_node_count(&self) -> usize {
        self.inner.history_tree().len()
    }

    /// Check whether the history tree is empty (no operations applied).
    fn history_is_empty(&self) -> bool {
        self.inner.history_tree().is_empty()
    }

    // --- Streaming engine methods (memory-efficient for large files) ---

    /// Count lines matching a regex in the original data.
    /// Uses ripgrep's SIMD-accelerated searcher on compressed chunks.
    fn count_matches(&self, pattern: &str) -> PyResult<usize> {
        Ok(crate::engine::fast::count_chunk_matches(
            self.inner.storage(),
            &self.inner.index,
            pattern,
        )?)
    }

    /// Count matches directly on a file on disk (fastest path, no import needed).
    #[staticmethod]
    fn count_file_matches(path: &str, pattern: &str) -> PyResult<usize> {
        Ok(crate::engine::fast::count_file_matches(
            &PathBuf::from(path),
            pattern,
        )?)
    }

    /// Search a file on disk for matching lines using ripgrep's searcher.
    /// Returns list of (line_number, content). No import needed.
    #[staticmethod]
    fn search_file(
        path: &str,
        pattern: &str,
        max_results: usize,
    ) -> PyResult<Vec<(u64, String)>> {
        Ok(crate::engine::fast::search_file(
            &PathBuf::from(path),
            pattern,
            max_results,
        )?)
    }

    /// Stream-filter original data to a file without loading all lines into memory.
    /// Returns the number of matching lines written.
    fn stream_filter_to_file(&self, pattern: &str, keep: bool, output: &str) -> PyResult<usize> {
        Ok(self
            .inner
            .processor()
            .filter_to_file(pattern, keep, &PathBuf::from(output), None)?)
    }

    /// Stream-replace in original data and write to a file.
    /// Returns the number of lines modified.
    fn stream_replace_to_file(
        &self,
        pattern: &str,
        replacement: &str,
        output: &str,
    ) -> PyResult<usize> {
        Ok(self
            .inner
            .processor()
            .replace_to_file(pattern, replacement, &PathBuf::from(output), None)?)
    }

    /// Search original data for lines matching a pattern using ripgrep's searcher.
    /// Returns list of (line_number, line_content) tuples.
    fn stream_search(&self, pattern: &str, max_results: usize) -> PyResult<Vec<(usize, String)>> {
        Ok(crate::engine::fast::search_chunks(
            self.inner.storage(),
            &self.inner.index,
            pattern,
            max_results,
        )?)
    }

    /// Parallel search across all chunks. Faster but results are collected then sorted.
    /// Returns list of (line_number, line_content) tuples.
    fn parallel_search(
        &self,
        pattern: &str,
        max_results: usize,
    ) -> PyResult<Vec<(usize, String)>> {
        Ok(self.inner.processor().parallel_search(pattern, max_results)?)
    }

    /// Export original data to a file, streaming chunk-by-chunk.
    /// Returns the number of lines written.
    fn stream_export(&self, output: &str) -> PyResult<usize> {
        Ok(self
            .inner
            .processor()
            .export_to_file(&PathBuf::from(output), None)?)
    }

    /// Compute statistics over the log without loading all data into memory.
    fn stats(&self) -> PyResult<PyLogStats> {
        let s = self.inner.processor().stats()?;
        Ok(PyLogStats {
            total_lines: s.total_lines,
            total_bytes: s.total_bytes,
            avg_line_len: s.avg_line_len,
            max_line_len: s.max_line_len,
            min_line_len: s.min_line_len,
            chunk_count: s.chunk_count,
        })
    }

    // --- Collector methods (read-only terminal operations) ---

    /// Count lines in current state, optionally filtered by regex.
    /// Returns int.
    #[pyo3(signature = (pattern=None))]
    fn collect_count(&mut self, pattern: Option<&str>) -> PyResult<usize> {
        let c = Collector::Count {
            pattern: pattern.map(|s| s.to_string()),
        };
        match self.inner.collect(&c)? {
            CollectResult::Count(n) => Ok(n),
            _ => unreachable!(),
        }
    }

    /// Group lines by a regex capture group, return {group_value: count}.
    /// `group_index` is the 1-based capture group number.
    fn collect_group_count<'py>(
        &mut self,
        py: Python<'py>,
        pattern: &str,
        group_index: usize,
    ) -> PyResult<Bound<'py, PyDict>> {
        let c = Collector::GroupCount {
            pattern: pattern.to_string(),
            group_index,
        };
        match self.inner.collect(&c)? {
            CollectResult::GroupCount(pairs) => {
                let dict = PyDict::new(py);
                for (k, v) in pairs {
                    dict.set_item(k, v)?;
                }
                Ok(dict)
            }
            _ => unreachable!(),
        }
    }

    /// Top-N most frequent values of a regex capture group.
    /// Returns list of (value, count) tuples sorted by count desc.
    fn collect_top_n(
        &mut self,
        pattern: &str,
        group_index: usize,
        n: usize,
    ) -> PyResult<Vec<(String, usize)>> {
        let c = Collector::TopN {
            pattern: pattern.to_string(),
            group_index,
            n,
        };
        match self.inner.collect(&c)? {
            CollectResult::TopN(pairs) => Ok(pairs),
            _ => unreachable!(),
        }
    }

    /// Collect distinct values of a regex capture group.
    /// Returns sorted list of unique strings.
    fn collect_unique(&mut self, pattern: &str, group_index: usize) -> PyResult<Vec<String>> {
        let c = Collector::Unique {
            pattern: pattern.to_string(),
            group_index,
        };
        match self.inner.collect(&c)? {
            CollectResult::Unique(vals) => Ok(vals),
            _ => unreachable!(),
        }
    }

    /// Compute numeric statistics from a regex capture group.
    /// The captured text is parsed as float. Returns dict with
    /// keys: count, sum, min, max, avg.
    fn collect_numeric_stats<'py>(
        &mut self,
        py: Python<'py>,
        pattern: &str,
        group_index: usize,
    ) -> PyResult<Bound<'py, PyDict>> {
        let c = Collector::NumericStats {
            pattern: pattern.to_string(),
            group_index,
        };
        match self.inner.collect(&c)? {
            CollectResult::NumericStats {
                count,
                sum,
                min,
                max,
                avg,
            } => {
                let dict = PyDict::new(py);
                dict.set_item("count", count)?;
                dict.set_item("sum", sum)?;
                dict.set_item("min", min)?;
                dict.set_item("max", max)?;
                dict.set_item("avg", avg)?;
                Ok(dict)
            }
            _ => unreachable!(),
        }
    }

    /// Compute line-length statistics over current state.
    /// Returns dict with keys: count, total_bytes, avg_len, max_len, min_len.
    fn collect_line_stats<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let c = Collector::LineStats;
        match self.inner.collect(&c)? {
            CollectResult::LineStats {
                count,
                total_bytes,
                avg_len,
                max_len,
                min_len,
            } => {
                let dict = PyDict::new(py);
                dict.set_item("count", count)?;
                dict.set_item("total_bytes", total_bytes)?;
                dict.set_item("avg_len", avg_len)?;
                dict.set_item("max_len", max_len)?;
                dict.set_item("min_len", min_len)?;
                Ok(dict)
            }
            _ => unreachable!(),
        }
    }

    // --- Collector methods on original data (ignore operations) ---

    /// Run a collector on the original data, ignoring any applied operations.
    /// Count lines, optionally filtered by a regex.
    fn collect_original_count(&self, pattern: Option<&str>) -> PyResult<usize> {
        let c = Collector::Count {
            pattern: pattern.map(|s| s.to_string()),
        };
        match self.inner.collect_original(&c)? {
            CollectResult::Count(n) => Ok(n),
            _ => unreachable!(),
        }
    }

    /// Group original lines by a regex capture group, return {group_value: count}.
    fn collect_original_group_count<'py>(
        &self,
        py: Python<'py>,
        pattern: &str,
        group_index: usize,
    ) -> PyResult<Bound<'py, PyDict>> {
        let c = Collector::GroupCount {
            pattern: pattern.to_string(),
            group_index,
        };
        match self.inner.collect_original(&c)? {
            CollectResult::GroupCount(pairs) => {
                let dict = PyDict::new(py);
                for (k, v) in pairs {
                    dict.set_item(k, v)?;
                }
                Ok(dict)
            }
            _ => unreachable!(),
        }
    }

    /// Top-N most frequent values of a capture group in original data.
    fn collect_original_top_n(
        &self,
        pattern: &str,
        group_index: usize,
        n: usize,
    ) -> PyResult<Vec<(String, usize)>> {
        let c = Collector::TopN {
            pattern: pattern.to_string(),
            group_index,
            n,
        };
        match self.inner.collect_original(&c)? {
            CollectResult::TopN(pairs) => Ok(pairs),
            _ => unreachable!(),
        }
    }

    /// Distinct values of a capture group in original data.
    fn collect_original_unique(
        &self,
        pattern: &str,
        group_index: usize,
    ) -> PyResult<Vec<String>> {
        let c = Collector::Unique {
            pattern: pattern.to_string(),
            group_index,
        };
        match self.inner.collect_original(&c)? {
            CollectResult::Unique(vals) => Ok(vals),
            _ => unreachable!(),
        }
    }

    /// Numeric statistics from a capture group in original data.
    fn collect_original_numeric_stats<'py>(
        &self,
        py: Python<'py>,
        pattern: &str,
        group_index: usize,
    ) -> PyResult<Bound<'py, PyDict>> {
        let c = Collector::NumericStats {
            pattern: pattern.to_string(),
            group_index,
        };
        match self.inner.collect_original(&c)? {
            CollectResult::NumericStats {
                count,
                sum,
                min,
                max,
                avg,
            } => {
                let dict = PyDict::new(py);
                dict.set_item("count", count)?;
                dict.set_item("sum", sum)?;
                dict.set_item("min", min)?;
                dict.set_item("max", max)?;
                dict.set_item("avg", avg)?;
                Ok(dict)
            }
            _ => unreachable!(),
        }
    }

    /// Line-length statistics over original data.
    fn collect_original_line_stats<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let c = Collector::LineStats;
        match self.inner.collect_original(&c)? {
            CollectResult::LineStats {
                count,
                total_bytes,
                avg_len,
                max_len,
                min_len,
            } => {
                let dict = PyDict::new(py);
                dict.set_item("count", count)?;
                dict.set_item("total_bytes", total_bytes)?;
                dict.set_item("avg_len", avg_len)?;
                dict.set_item("max_len", max_len)?;
                dict.set_item("min_len", min_len)?;
                Ok(dict)
            }
            _ => unreachable!(),
        }
    }
}

/// Python wrapper for repository metadata.
#[pyclass(name = "RepoMetadata", from_py_object)]
#[derive(Clone)]
pub struct PyRepoMetadata {
    inner: crate::repo::RepoMetadata,
}

#[pymethods]
impl PyRepoMetadata {
    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }

    #[getter]
    fn source_name(&self) -> &str {
        &self.inner.source_name
    }

    #[getter]
    fn original_size(&self) -> u64 {
        self.inner.original_size
    }

    #[getter]
    fn original_line_count(&self) -> usize {
        self.inner.original_line_count
    }

    #[getter]
    fn created_at(&self) -> String {
        self.inner.created_at.to_rfc3339()
    }

    #[getter]
    fn description(&self) -> Option<String> {
        self.inner.description.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "RepoMetadata(source='{}', lines={}, size={})",
            self.inner.source_name, self.inner.original_line_count, self.inner.original_size
        )
    }
}

/// Python wrapper for an operation history record.
#[pyclass(name = "OperationRecord", from_py_object)]
#[derive(Clone)]
pub struct PyOperationRecord {
    #[pyo3(get)]
    id: usize,
    #[pyo3(get)]
    description: String,
    #[pyo3(get)]
    applied_at: String,
}

#[pymethods]
impl PyOperationRecord {
    fn __repr__(&self) -> String {
        format!(
            "OperationRecord(id={}, desc='{}', at='{}')",
            self.id, self.description, self.applied_at
        )
    }
}

/// Log statistics computed in streaming fashion.
#[pyclass(name = "LogStats")]
pub struct PyLogStats {
    #[pyo3(get)]
    total_lines: usize,
    #[pyo3(get)]
    total_bytes: usize,
    #[pyo3(get)]
    avg_line_len: f64,
    #[pyo3(get)]
    max_line_len: usize,
    #[pyo3(get)]
    min_line_len: usize,
    #[pyo3(get)]
    chunk_count: usize,
}

#[pymethods]
impl PyLogStats {
    fn __repr__(&self) -> String {
        format!(
            "LogStats(lines={}, bytes={}, avg_len={:.1}, chunks={})",
            self.total_lines, self.total_bytes, self.avg_line_len, self.chunk_count
        )
    }
}

/// Python wrapper for Workspace — manages multiple named repos.
#[pyclass(name = "Workspace")]
pub struct PyWorkspace {
    inner: Workspace,
}

#[pymethods]
impl PyWorkspace {
    /// Open (or initialize) a workspace at the given root directory.
    #[new]
    fn new(root: &str) -> PyResult<Self> {
        let ws = Workspace::open(&PathBuf::from(root))?;
        ws.migrate_if_needed()?;
        Ok(Self { inner: ws })
    }

    /// Whether the workspace has been initialized.
    fn is_initialized(&self) -> bool {
        self.inner.is_initialized()
    }

    /// List all repo names.
    fn list(&self) -> PyResult<Vec<String>> {
        Ok(self.inner.list()?)
    }

    /// Get the active repo name.
    fn active(&self) -> PyResult<String> {
        Ok(self.inner.active()?)
    }

    /// Set the active repo name.
    fn set_active(&self, name: &str) -> PyResult<()> {
        Ok(self.inner.set_active(name)?)
    }

    /// Check if a named repo exists.
    fn has_repo(&self, name: &str) -> bool {
        self.inner.has_repo(name)
    }

    /// Open a named repo.
    fn open_repo(&self, name: &str) -> PyResult<PyLogRepo> {
        Ok(PyLogRepo {
            inner: self.inner.open_repo(name)?,
        })
    }

    /// Open the currently active repo.
    fn open_active(&self) -> PyResult<PyLogRepo> {
        Ok(PyLogRepo {
            inner: self.inner.open_active()?,
        })
    }

    /// Import a file into a new named repo.
    #[pyo3(signature = (source_file, name="default"))]
    fn import_file(&self, source_file: &str, name: &str) -> PyResult<PyLogRepo> {
        Ok(PyLogRepo {
            inner: self
                .inner
                .import_file(name, &PathBuf::from(source_file))?,
        })
    }

    /// Import text into a new named repo.
    #[pyo3(signature = (text, source_name, name="default"))]
    fn import_text(&self, text: &str, source_name: &str, name: &str) -> PyResult<PyLogRepo> {
        Ok(PyLogRepo {
            inner: self
                .inner
                .import_bytes(name, text.as_bytes(), source_name.to_string())?,
        })
    }

    /// Clone a repo under a new name.
    fn clone_repo(&self, src: &str, dst: &str) -> PyResult<PyLogRepo> {
        Ok(PyLogRepo {
            inner: self.inner.clone_repo(src, dst)?,
        })
    }

    /// Remove a named repo.
    fn remove_repo(&self, name: &str) -> PyResult<()> {
        Ok(self.inner.remove_repo(name)?)
    }

    /// Merge multiple source repos (in order) into a new target repo.
    ///
    /// Each source's current state (after its operations) is concatenated in
    /// the order given.  The target repo must not already exist.
    fn merge_repos(&self, sources: Vec<String>, target: &str) -> PyResult<PyLogRepo> {
        let source_refs: Vec<&str> = sources.iter().map(|s| s.as_str()).collect();
        Ok(PyLogRepo {
            inner: self.inner.merge_repos(&source_refs, target)?,
        })
    }

    /// Workspace root path.
    fn root(&self) -> String {
        self.inner.root().to_string_lossy().to_string()
    }
}
