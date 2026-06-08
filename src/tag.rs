use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{LogAnalyzerError, Result};

/// A named tag referencing line ranges in a specific repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    /// User-visible tag name (e.g. "errors", "tag_1").
    pub name: String,
    /// Sorted, non-overlapping inclusive line ranges (start_line, end_line).
    /// Line numbers are 0-based and refer to the current state at creation time.
    pub ranges: Vec<(usize, usize)>,
    /// When the tag was created.
    pub created_at: DateTime<Utc>,
}

/// Reference to a tag scope recorded on a HistoryNode.
///
/// Stores the tag name and the resolved line ranges at the time the
/// operation was applied, so downstream node operations (merge, diff,
/// replay) can reconstruct the scope context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagScopeRef {
    pub tag_name: String,
    /// Line ranges (0-based, inclusive) at the time of operation.
    pub ranges: Vec<(usize, usize)>,
}

/// Persistent tag store for a workspace.
///
/// Tags are stored per-repo in a single JSON file at
/// `{workspace_root}/.log_analyzer/tags.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagStore {
    /// repo_name → tags
    pub repos: HashMap<String, Vec<Tag>>,
}

impl TagStore {
    /// Create an empty tag store.
    pub fn new() -> Self {
        Self {
            repos: HashMap::new(),
        }
    }

    /// Load the tag store from disk. Returns an empty store if the file
    /// doesn't exist or can't be parsed.
    pub fn load(workspace_root: &Path) -> Self {
        let path = tags_path(workspace_root);
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(store) = serde_json::from_str::<TagStore>(&data) {
                    return store;
                }
            }
        }
        Self::new()
    }

    /// Save the tag store to disk.
    pub fn save(&self, workspace_root: &Path) -> Result<()> {
        let path = tags_path(workspace_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                LogAnalyzerError::Io(e)
            })?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// Get all tags for a repo. Returns empty slice if repo has no tags.
    pub fn get_tags(&self, repo_name: &str) -> &[Tag] {
        self.repos.get(repo_name).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Add a tag for a repo. If a tag with the same name already exists,
    /// it is replaced.
    pub fn add_tag(&mut self, repo_name: &str, tag: Tag) {
        let tags = self.repos.entry(repo_name.to_string()).or_default();
        // Replace existing tag with same name
        tags.retain(|t| t.name != tag.name);
        tags.push(tag);
    }

    /// Remove a tag by name.
    pub fn remove_tag(&mut self, repo_name: &str, name: &str) -> bool {
        if let Some(tags) = self.repos.get_mut(repo_name) {
            let len_before = tags.len();
            tags.retain(|t| t.name != name);
            tags.len() < len_before
        } else {
            false
        }
    }

    /// Rename a tag.
    pub fn rename_tag(&mut self, repo_name: &str, old_name: &str, new_name: &str) -> bool {
        if let Some(tags) = self.repos.get_mut(repo_name) {
            if let Some(tag) = tags.iter_mut().find(|t| t.name == old_name) {
                tag.name = new_name.to_string();
                return true;
            }
        }
        false
    }

    /// Find a tag by name.
    pub fn find_tag(&self, repo_name: &str, name: &str) -> Option<&Tag> {
        self.repos
            .get(repo_name)
            .and_then(|tags| tags.iter().find(|t| t.name == name))
    }

    /// Get next auto-numbered tag name (tag_1, tag_2, ...).
    pub fn next_auto_name(&self, repo_name: &str) -> String {
        let tags = self.get_tags(repo_name);
        let mut max_n: usize = 0;
        for tag in tags {
            if let Some(rest) = tag.name.strip_prefix("tag_") {
                if let Ok(n) = rest.parse::<usize>() {
                    if n > max_n {
                        max_n = n;
                    }
                }
            }
        }
        format!("tag_{}", max_n + 1)
    }

    /// Build a TagScopeRef from a tag name for the given repo.
    pub fn make_scope(&self, repo_name: &str, tag_name: &str) -> Option<TagScopeRef> {
        self.find_tag(repo_name, tag_name).map(|tag| TagScopeRef {
            tag_name: tag.name.clone(),
            ranges: tag.ranges.clone(),
        })
    }
}

impl Default for TagStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the path to tags.json in the workspace's .log_analyzer directory.
fn tags_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".log_analyzer").join("tags.json")
}

/// Filter a slice of lines to only include those within the given ranges.
///
/// `ranges` are (start, end) inclusive 0-based line indices.
/// The returned vec contains only the lines within those ranges, in order.
pub fn filter_lines_by_ranges(lines: &[String], ranges: &[(usize, usize)]) -> Vec<String> {
    let mut result = Vec::new();
    for &(start, end) in ranges {
        let start = start.min(lines.len());
        let end = (end + 1).min(lines.len()); // +1 because ranges are inclusive
        result.extend_from_slice(&lines[start..end]);
    }
    result
}

/// Compute the set difference: lines in `base` that are NOT in `subtrahend`.
/// Comparison is by exact string match.
pub fn subtract_line_sets(base: &[String], subtrahend: &[String]) -> Vec<String> {
    use std::collections::HashSet;
    let sub_set: HashSet<&String> = subtrahend.iter().collect();
    base.iter()
        .filter(|line| !sub_set.contains(line))
        .cloned()
        .collect()
}

/// Compute the set union: all unique lines from all sources.
pub fn union_line_sets(sources: &[Vec<String>]) -> Vec<String> {
    use std::collections::HashSet;
    let mut seen: HashSet<&String> = HashSet::new();
    let mut result = Vec::new();
    for lines in sources {
        for line in lines {
            if seen.insert(line) {
                result.push(line.clone());
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_store_empty() {
        let store = TagStore::new();
        assert!(store.get_tags("test").is_empty());
    }

    #[test]
    fn test_add_and_get_tags() {
        let mut store = TagStore::new();
        let tag = Tag {
            name: "errors".into(),
            ranges: vec![(0, 10), (20, 30)],
            created_at: Utc::now(),
        };
        store.add_tag("myrepo", tag);
        assert_eq!(store.get_tags("myrepo").len(), 1);
        assert_eq!(store.get_tags("myrepo")[0].name, "errors");
    }

    #[test]
    fn test_add_replaces_same_name() {
        let mut store = TagStore::new();
        store.add_tag(
            "repo",
            Tag {
                name: "t".into(),
                ranges: vec![(0, 5)],
                created_at: Utc::now(),
            },
        );
        store.add_tag(
            "repo",
            Tag {
                name: "t".into(),
                ranges: vec![(10, 20)],
                created_at: Utc::now(),
            },
        );
        let tags = store.get_tags("repo");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].ranges, vec![(10, 20)]);
    }

    #[test]
    fn test_remove_tag() {
        let mut store = TagStore::new();
        store.add_tag(
            "repo",
            Tag {
                name: "t".into(),
                ranges: vec![(0, 5)],
                created_at: Utc::now(),
            },
        );
        assert!(store.remove_tag("repo", "t"));
        assert!(store.get_tags("repo").is_empty());
        assert!(!store.remove_tag("repo", "nonexistent"));
    }

    #[test]
    fn test_rename_tag() {
        let mut store = TagStore::new();
        store.add_tag(
            "repo",
            Tag {
                name: "old".into(),
                ranges: vec![(0, 5)],
                created_at: Utc::now(),
            },
        );
        assert!(store.rename_tag("repo", "old", "new"));
        assert_eq!(store.get_tags("repo")[0].name, "new");
        assert!(!store.rename_tag("repo", "nope", "x"));
    }

    #[test]
    fn test_next_auto_name() {
        let mut store = TagStore::new();
        assert_eq!(store.next_auto_name("repo"), "tag_1");

        store.add_tag(
            "repo",
            Tag {
                name: "tag_1".into(),
                ranges: vec![(0, 1)],
                created_at: Utc::now(),
            },
        );
        assert_eq!(store.next_auto_name("repo"), "tag_2");

        store.add_tag(
            "repo",
            Tag {
                name: "custom".into(),
                ranges: vec![(5, 10)],
                created_at: Utc::now(),
            },
        );
        assert_eq!(store.next_auto_name("repo"), "tag_2");
    }

    #[test]
    fn test_filter_lines_by_ranges() {
        let lines: Vec<String> = (0..10).map(|i| format!("line{}", i)).collect();
        let ranges = vec![(1, 2), (5, 7)];
        let filtered = filter_lines_by_ranges(&lines, &ranges);
        assert_eq!(filtered, vec!["line1", "line2", "line5", "line6", "line7"]);
    }

    #[test]
    fn test_subtract_line_sets() {
        let a = vec!["a".into(), "b".into(), "c".into()];
        let b = vec!["b".into()];
        let diff = subtract_line_sets(&a, &b);
        assert_eq!(diff, vec!["a", "c"]);
    }

    #[test]
    fn test_union_line_sets() {
        let a = vec!["a".into(), "b".into()];
        let b = vec!["b".into(), "c".into()];
        let union = union_line_sets(&[a, b]);
        assert_eq!(union.len(), 3);
        assert!(union.contains(&"a".to_string()));
        assert!(union.contains(&"b".to_string()));
        assert!(union.contains(&"c".to_string()));
    }

    #[test]
    fn test_make_scope() {
        let mut store = TagStore::new();
        store.add_tag(
            "repo",
            Tag {
                name: "scope1".into(),
                ranges: vec![(10, 50)],
                created_at: Utc::now(),
            },
        );
        let scope = store.make_scope("repo", "scope1").unwrap();
        assert_eq!(scope.tag_name, "scope1");
        assert_eq!(scope.ranges, vec![(10, 50)]);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let mut store = TagStore::new();
        store.add_tag(
            "repo",
            Tag {
                name: "t".into(),
                ranges: vec![(0, 5)],
                created_at: Utc::now(),
            },
        );
        let json = serde_json::to_string(&store).unwrap();
        let restored: TagStore = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.get_tags("repo")[0].name, "t");
    }
}
