mod crud;
mod filter;
mod replace;

pub use crud::{DeleteLines, InsertLines, ModifyLine};
pub use filter::Filter;
pub use replace::Replace;

use chrono::{DateTime, Utc};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::engine::Collector;
use crate::error::Result;

/// The set operation mode for merging multiple source nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeMode {
    /// Set union — all unique lines from all sources.
    Union,
    /// Set intersection — only lines present in every source.
    Intersection,
    /// Set subtraction — first source minus all other sources.
    Subtract,
    /// Symmetric difference — lines present in an odd number of sources.
    Xor,
}

/// Default merge mode for backward compatibility (old serialized Merge nodes
/// don't have a `mode` field).
fn default_merge_mode() -> MergeMode {
    MergeMode::Union
}

/// Represents a reversible operation on log lines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    /// Filter lines by regex pattern. `keep=true` keeps matches, `keep=false` removes them.
    Filter {
        pattern: String,
        keep: bool,
    },
    /// Replace matching text with a replacement string.
    Replace {
        pattern: String,
        replacement: String,
    },
    /// Delete specific lines by index.
    DeleteLines {
        line_indices: Vec<usize>,
    },
    /// Insert lines after a given position.
    InsertLines {
        after_line: usize,
        content: Vec<String>,
    },
    /// Modify a single line.
    ModifyLine {
        line_index: usize,
        new_content: String,
    },
    /// Merge: set operation on line sets from multiple source nodes.
    /// Handled at LogRepo level — see `LogRepo::merge_nodes()`.
    Merge {
        sources: Vec<usize>,
        #[serde(default = "default_merge_mode")]
        mode: MergeMode,
    },
    /// Subtract: lines in `base` node minus lines in `subtrahend` node.
    /// Handled at LogRepo level — see `LogRepo::subtract_nodes()`.
    Subtract {
        base: usize,
        subtrahend: usize,
    },
    /// Replay: re-apply a source node's operation at a different tree position.
    /// Handled at LogRepo level — see `LogRepo::replay_node_at()`.
    Replay {
        source_node_id: usize,
    },
    /// Collect: run a collector on current state and replace log content
    /// with the formatted collect result as text lines.
    Collect {
        collector: Collector,
    },
}

/// Stored inverse data for undoing an operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InverseData {
    /// For filter: the removed lines with their original positions.
    FilterInverse {
        removed: Vec<(usize, String)>,
    },
    /// For replace: original lines that were modified.
    ReplaceInverse {
        originals: Vec<(usize, String)>,
    },
    /// For delete: the deleted lines with their original positions.
    DeleteInverse {
        deleted: Vec<(usize, String)>,
    },
    /// For insert: the count of inserted lines and position.
    InsertInverse {
        after_line: usize,
        count: usize,
    },
    /// For modify: the original content.
    ModifyInverse {
        line_index: usize,
        original_content: String,
    },
    /// For merge: the line sets from each source node (for undo reconstruction).
    MergeInverse {
        source_line_sets: Vec<Vec<String>>,
    },
    /// For subtract: the lines removed from the base set.
    SubtractInverse {
        removed: Vec<(usize, String)>,
    },
    /// For replay: the inverse data from the replayed operation.
    ReplayInverse {
        inner: Box<InverseData>,
    },
    /// For collect: the original lines before collection (for undo).
    CollectInverse {
        previous_lines: Vec<String>,
    },
}

/// A recorded operation with its inverse for undo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRecord {
    pub id: usize,
    pub operation: Operation,
    pub inverse: InverseData,
    pub applied_at: DateTime<Utc>,
}

impl Operation {
    /// Apply this operation to lines and return the result.
    pub fn apply(&self, lines: Vec<String>) -> Result<Vec<String>> {
        match self {
            Operation::Filter { pattern, keep } => Filter::apply(lines, pattern, *keep),
            Operation::Replace {
                pattern,
                replacement,
            } => Replace::apply(lines, pattern, replacement),
            Operation::DeleteLines { line_indices } => DeleteLines::apply(lines, line_indices),
            Operation::InsertLines {
                after_line,
                content,
            } => InsertLines::apply(lines, *after_line, content),
            Operation::ModifyLine {
                line_index,
                new_content,
            } => ModifyLine::apply(lines, *line_index, new_content),
            Operation::Merge { .. } | Operation::Subtract { .. } | Operation::Replay { .. } => {
                Err(crate::error::LogAnalyzerError::Operator(
                    "Merge/Subtract/Replay must be applied via LogRepo methods".into(),
                ))
            }
            Operation::Collect { collector } => {
                let result = crate::engine::collector::execute_on_lines(collector, &lines)?;
                Ok(result.to_lines())
            }
        }
    }

    /// Apply this operation and also compute the inverse data for undo.
    pub fn apply_with_inverse(&self, lines: Vec<String>) -> Result<(Vec<String>, InverseData)> {
        match self {
            Operation::Filter { pattern, keep } => {
                Filter::apply_with_inverse(lines, pattern, *keep)
            }
            Operation::Replace {
                pattern,
                replacement,
            } => Replace::apply_with_inverse(lines, pattern, replacement),
            Operation::DeleteLines { line_indices } => {
                DeleteLines::apply_with_inverse(lines, line_indices)
            }
            Operation::InsertLines {
                after_line,
                content,
            } => InsertLines::apply_with_inverse(lines, *after_line, content),
            Operation::ModifyLine {
                line_index,
                new_content,
            } => ModifyLine::apply_with_inverse(lines, *line_index, new_content),
            Operation::Merge { .. } | Operation::Subtract { .. } | Operation::Replay { .. } => {
                Err(crate::error::LogAnalyzerError::Operator(
                    "Merge/Subtract/Replay must be applied via LogRepo methods".into(),
                ))
            }
            Operation::Collect { collector } => {
                let previous_lines = lines.clone();
                let result = crate::engine::collector::execute_on_lines(collector, &lines)?;
                let new_lines = result.to_lines();
                let inverse = InverseData::CollectInverse { previous_lines };
                Ok((new_lines, inverse))
            }
        }
    }

    /// Get a human-readable description of this operation.
    pub fn describe(&self) -> String {
        match self {
            Operation::Filter { pattern, keep } => {
                if *keep {
                    format!("filter keep /{}/", pattern)
                } else {
                    format!("filter remove /{}/", pattern)
                }
            }
            Operation::Replace {
                pattern,
                replacement,
            } => {
                format!("replace /{}/ -> \"{}\"", pattern, replacement)
            }
            Operation::DeleteLines { line_indices } => {
                if line_indices.len() <= 5 {
                    format!("delete lines {:?}", line_indices)
                } else {
                    format!("delete {} lines", line_indices.len())
                }
            }
            Operation::InsertLines {
                after_line,
                content,
            } => {
                format!("insert {} lines after line {}", content.len(), after_line)
            }
            Operation::ModifyLine {
                line_index,
                new_content: _,
            } => {
                format!("modify line {}", line_index)
            }
            Operation::Merge { sources, mode } => {
                let ids: Vec<String> = sources.iter().map(|s| s.to_string()).collect();
                let mode_str = match mode {
                    MergeMode::Union => "OR",
                    MergeMode::Intersection => "AND",
                    MergeMode::Subtract => "SUB",
                    MergeMode::Xor => "XOR",
                };
                format!("merge [{}] ({})", ids.join(", "), mode_str)
            }
            Operation::Subtract { base, subtrahend } => {
                format!("subtract node {} from node {}", subtrahend, base)
            }
            Operation::Replay { source_node_id } => {
                format!("replay node {}", source_node_id)
            }
            Operation::Collect { collector } => {
                format!("collect {}", collector.describe())
            }
        }
    }
}

/// Apply an operation in parallel across chunks of lines.
/// Used by filter and replace for large datasets.
pub fn parallel_apply<F>(lines: Vec<String>, chunk_size: usize, f: F) -> Vec<String>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if lines.len() < chunk_size * 2 {
        // Small dataset: sequential
        lines
            .into_iter()
            .filter_map(|line| f(&line))
            .collect()
    } else {
        // Large dataset: parallel
        lines
            .into_par_iter()
            .filter_map(|line| f(&line))
            .collect()
    }
}
