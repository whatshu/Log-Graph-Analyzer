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

/// Compute the set intersection: only lines present in ALL sources.
/// Order is preserved from the first source.
pub fn intersect_line_sets(sources: &[Vec<String>]) -> Vec<String> {
    use std::collections::HashSet;
    if sources.is_empty() {
        return Vec::new();
    }
    // Build the intersection set starting from the first source
    let mut result_set: HashSet<&String> = sources[0].iter().collect();
    for lines in &sources[1..] {
        let current_set: HashSet<&String> = lines.iter().collect();
        result_set = result_set.intersection(&current_set).cloned().collect();
    }
    // Preserve order from the first source
    let mut result = Vec::new();
    let mut emitted: HashSet<&String> = HashSet::new();
    for line in &sources[0] {
        if result_set.contains(line) && emitted.insert(line) {
            result.push(line.clone());
        }
    }
    result
}

/// Compute the symmetric difference (XOR): lines appearing in an odd number
/// of sources. Each source is treated as a set (duplicates within a source
/// are counted once).
pub fn xor_line_sets(sources: &[Vec<String>]) -> Vec<String> {
    use std::collections::HashMap;
    let mut count_map: HashMap<&String, usize> = HashMap::new();
    let mut order: Vec<&String> = Vec::new();
    for lines in sources {
        let mut seen = std::collections::HashSet::new();
        for line in lines {
            if seen.insert(line) {
                if !count_map.contains_key(line) {
                    order.push(line);
                }
                *count_map.entry(line).or_insert(0) += 1;
            }
        }
    }
    // Collect lines with odd count, preserving first-seen order
    order
        .into_iter()
        .filter(|line| count_map.get(line).copied().unwrap_or(0) % 2 == 1)
        .cloned()
        .collect()
}

// ── LCA-ordered merge functions ──
//
// These functions merge source line sets while preserving the relative
// ordering of lines as they appear in the lowest common ancestor (LCA).
// Instead of concatenating and deduplicating source results, they walk
// through the LCA lines and determine inclusion based on the merge mode.
// Unmatched source lines (from insertions/replacements) are appended at
// the end in source order.

/// Build frequency maps from source line sets.
fn build_freq_maps(sources: &[Vec<String>]) -> Vec<HashMap<&str, usize>> {
    sources
        .iter()
        .map(|lines| {
            let mut map = HashMap::new();
            for line in lines {
                *map.entry(line.as_str()).or_insert(0) += 1;
            }
            map
        })
        .collect()
}

/// Merge sources with LCA ordering — Union mode.
///
/// Walks through LCA lines in order. For each line, if it appears in at
/// least one source (accounting for multiplicity), it is included and
/// consumed from the first matching source. Remaining unmatched source
/// lines are appended at the end.
pub fn merge_union_by_lca(lca: &[String], sources: &[Vec<String>]) -> Vec<String> {
    let mut freq_maps = build_freq_maps(sources);
    let mut result = Vec::new();

    for lca_line in lca {
        let key = lca_line.as_str();
        let mut found = false;
        for freq in &mut freq_maps {
            if let Some(count) = freq.get_mut(key) {
                if *count > 0 {
                    *count -= 1;
                    found = true;
                    break; // consume from first source that has it
                }
            }
        }
        if found {
            result.push(lca_line.clone());
        }
    }

    // Append any remaining unmatched lines (from insertions/replacements)
    for freq in &freq_maps {
        for (line, count) in freq.iter() {
            for _ in 0..*count {
                result.push(line.to_string());
            }
        }
    }

    result
}

/// Merge sources with LCA ordering — Intersection mode.
///
/// Walks through LCA lines. For each line, it is included only if it
/// appears in ALL sources (accounting for multiplicity). When included,
/// one copy is consumed from each source.
pub fn merge_intersection_by_lca(lca: &[String], sources: &[Vec<String>]) -> Vec<String> {
    if sources.is_empty() {
        return Vec::new();
    }
    let mut freq_maps = build_freq_maps(sources);
    let mut result = Vec::new();

    for lca_line in lca {
        let key = lca_line.as_str();
        let all_have = freq_maps
            .iter()
            .all(|freq| freq.get(key).copied().unwrap_or(0) > 0);
        if all_have {
            result.push(lca_line.clone());
            for freq in &mut freq_maps {
                if let Some(count) = freq.get_mut(key) {
                    *count -= 1;
                }
            }
        }
    }

    result
}

/// Merge sources with LCA ordering — Subtract mode.
///
/// `sources[0]` is the base; all others are subtrahends (treated as sets).
/// Walks through LCA lines. Each line is included if it appears in the base
/// (with multiplicity) but NOT in any subtrahend set.
pub fn merge_subtract_by_lca(
    lca: &[String],
    base: &[String],
    subtrahends: &[Vec<String>],
) -> Vec<String> {
    use std::collections::HashSet;

    let mut base_freq: HashMap<&str, usize> = HashMap::new();
    for line in base {
        *base_freq.entry(line.as_str()).or_insert(0) += 1;
    }

    // Build union of all subtrahend sets
    let mut sub_set: HashSet<&str> = HashSet::new();
    for sub in subtrahends {
        for line in sub {
            sub_set.insert(line.as_str());
        }
    }

    let mut result = Vec::new();
    for lca_line in lca {
        let key = lca_line.as_str();
        let in_base = base_freq.get(key).copied().unwrap_or(0) > 0;
        let in_sub = sub_set.contains(key);
        if in_base {
            // Consume from base regardless of inclusion — a line that is
            // excluded by the subtrahend should not later appear as unmatched.
            if let Some(count) = base_freq.get_mut(key) {
                *count -= 1;
            }
            if !in_sub {
                result.push(lca_line.clone());
            }
        }
    }

    // Append unmatched base lines
    for (line, count) in &base_freq {
        for _ in 0..*count {
            result.push(line.to_string());
        }
    }

    result
}

/// Merge sources with LCA ordering — XOR (symmetric difference) mode.
///
/// Each source is treated as a set (unique lines only). A line is included
/// if it appears in an odd number of source sets. The result preserves LCA
/// line ordering.
pub fn merge_xor_by_lca(lca: &[String], sources: &[Vec<String>]) -> Vec<String> {
    use std::collections::HashSet;

    // Count how many source sets contain each unique line
    let mut source_count: HashMap<&str, usize> = HashMap::new();
    for lines in sources {
        let mut seen = HashSet::new();
        for line in lines {
            if seen.insert(line.as_str()) {
                *source_count.entry(line.as_str()).or_insert(0) += 1;
            }
        }
    }

    // Determine which lines to include (odd count)
    let include: HashSet<&str> = source_count
        .iter()
        .filter(|(_, count)| *count % 2 == 1)
        .map(|(line, _)| *line)
        .collect();

    // Walk through LCA and include/exclude
    lca.iter()
        .filter(|line| include.contains(line.as_str()))
        .cloned()
        .collect()
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
    fn test_intersect_line_sets() {
        let a = vec!["a".into(), "b".into(), "c".into()];
        let b = vec!["b".into(), "c".into(), "d".into()];
        let inter = intersect_line_sets(&[a, b]);
        assert_eq!(inter.len(), 2);
        assert!(inter.contains(&"b".to_string()));
        assert!(inter.contains(&"c".to_string()));
    }

    #[test]
    fn test_intersect_line_sets_empty() {
        let a = vec!["x".into()];
        let b = vec!["y".into()];
        let inter = intersect_line_sets(&[a, b]);
        assert!(inter.is_empty());
    }

    #[test]
    fn test_intersect_line_sets_single_source() {
        let a = vec!["a".into(), "b".into()];
        let inter = intersect_line_sets(&[a]);
        assert_eq!(inter.len(), 2);
    }

    #[test]
    fn test_intersect_line_sets_empty_input() {
        let inter = intersect_line_sets(&[]);
        assert!(inter.is_empty());
    }

    #[test]
    fn test_xor_line_sets() {
        let a = vec!["a".into(), "b".into()];
        let b = vec!["b".into(), "c".into()];
        // a appears 1x (odd), b appears 2x (even), c appears 1x (odd) -> a, c
        let xor = xor_line_sets(&[a, b]);
        assert_eq!(xor.len(), 2);
        assert!(xor.contains(&"a".to_string()));
        assert!(xor.contains(&"c".to_string()));
    }

    #[test]
    fn test_xor_line_sets_three_sources() {
        let a = vec!["a".into(), "b".into()];
        let b = vec!["b".into(), "c".into()];
        let c = vec!["c".into(), "d".into(), "a".into()];
        // a: 2x (even), b: 2x (even), c: 2x (even), d: 1x (odd) -> d
        let xor = xor_line_sets(&[a, b, c]);
        assert_eq!(xor.len(), 1);
        assert!(xor.contains(&"d".to_string()));
    }

    #[test]
    fn test_xor_line_sets_single_source() {
        let a = vec!["a".into(), "b".into()];
        let xor = xor_line_sets(&[a]);
        assert_eq!(xor.len(), 2);
    }

    #[test]
    fn test_xor_line_sets_empty_input() {
        let xor = xor_line_sets(&[]);
        assert!(xor.is_empty());
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

    // ── LCA-ordered merge tests ──

    #[test]
    fn test_merge_union_by_lca_basic() {
        // Example from user: LCA has interleaved content
        let lca: Vec<String> = vec!["01", "02", "03", "01", "01", "02", "02", "03", "03"]
            .into_iter().map(|s| s.to_string()).collect();
        let s1: Vec<String> = vec!["01", "01", "01"]
            .into_iter().map(|s| s.to_string()).collect();
        let s2: Vec<String> = vec!["02", "02", "02"]
            .into_iter().map(|s| s.to_string()).collect();

        let result = merge_union_by_lca(&lca, &[s1, s2]);
        // Should preserve LCA order: 01, 02, 01, 01, 02, 02
        assert_eq!(result, vec!["01", "02", "01", "01", "02", "02"]);
    }

    #[test]
    fn test_merge_union_by_lca_overlap() {
        // Both sources have some of the same lines
        let lca: Vec<String> = vec!["a", "b", "a", "c", "b"]
            .into_iter().map(|s| s.to_string()).collect();
        let s1 = vec!["a".to_string(), "a".to_string(), "b".to_string()];
        let s2 = vec!["b".to_string(), "c".to_string()];

        let result = merge_union_by_lca(&lca, &[s1, s2]);
        // LCA order: a, b, a, c, b → both sources contain these
        assert_eq!(result, vec!["a", "b", "a", "c", "b"]);
    }

    #[test]
    fn test_merge_intersection_by_lca() {
        let lca: Vec<String> = vec!["a", "b", "c", "a", "b"]
            .into_iter().map(|s| s.to_string()).collect();
        let s1 = vec!["a".to_string(), "b".to_string(), "a".to_string()];
        let s2 = vec!["b".to_string(), "a".to_string()];

        let result = merge_intersection_by_lca(&lca, &[s1, s2]);
        // a: both have 2? s1 has 2, s2 has 1 → intersect at 1
        // b: s1 has 1, s2 has 1 → intersect at 1
        // LCA: a, b, c, a, b
        //   a: both have → include, consume 1 from each
        //   b: both have → include, consume 1 from each
        //   c: neither has enough → skip
        //   a: s1 has 1 left, s2 has 0 left → skip (s2 exhausted)
        //   b: s1 has 0 left → skip
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn test_merge_intersection_by_lca_empty() {
        let lca: Vec<String> = vec!["x", "y"]
            .into_iter().map(|s| s.to_string()).collect();
        let s1 = vec!["x".to_string()];
        let s2 = vec!["y".to_string()];

        let result = merge_intersection_by_lca(&lca, &[s1, s2]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_merge_subtract_by_lca() {
        let lca: Vec<String> = vec!["a", "b", "c", "a", "b"]
            .into_iter().map(|s| s.to_string()).collect();
        let base = vec!["a".to_string(), "a".to_string(), "c".to_string()];
        let sub = vec!["c".to_string()]; // subtrahend as set

        let result = merge_subtract_by_lca(&lca, &base, &[sub]);
        // LCA: a, b, c, a, b
        //   a: in base, not in sub → include (base freq a: 1 remaining)
        //   b: not in base → skip
        //   c: in base AND in sub → skip (base freq c consumed)
        //   a: in base (1 left) → include (base freq a: 0)
        //   b: not in base → skip
        assert_eq!(result, vec!["a", "a"]);
    }

    #[test]
    fn test_merge_xor_by_lca() {
        let lca: Vec<String> = vec!["a", "b", "c", "a", "d"]
            .into_iter().map(|s| s.to_string()).collect();
        let s1 = vec!["a".to_string(), "b".to_string()];
        let s2 = vec!["b".to_string(), "c".to_string(), "d".to_string()];

        let result = merge_xor_by_lca(&lca, &[s1, s2]);
        // a: 1 source → include
        // b: 2 sources → exclude
        // c: 1 source → include
        // d: 1 source → include
        // LCA order: a, b, c, a, d → filter by include set {a, c, d}
        assert_eq!(result, vec!["a", "c", "a", "d"]);
    }

    #[test]
    fn test_merge_xor_by_lca_three_sources() {
        let lca: Vec<String> = vec!["a", "b", "c"]
            .into_iter().map(|s| s.to_string()).collect();
        let s1 = vec!["a".to_string(), "b".to_string()];
        let s2 = vec!["b".to_string(), "c".to_string()];
        let s3 = vec!["c".to_string(), "a".to_string()];
        // a: 2 sources (s1, s3) → exclude
        // b: 2 sources (s1,s2) → exclude
        // c: 2 sources (s2,s3) → exclude
        let result = merge_xor_by_lca(&lca, &[s1, s2, s3]);
        assert!(result.is_empty());
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
