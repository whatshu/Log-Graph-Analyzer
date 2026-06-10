use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::operator::{InverseData, Operation};
use crate::tag::TagScopeRef;

/// A single node in the operation history tree.
///
/// Each node represents one applied operation (except root which is import).
/// Nodes support soft-delete — they can be marked deleted while remaining
/// in the tree, allowing patterns to be preserved for reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryNode {
    /// Unique node identifier.
    pub id: usize,
    /// Parent node ID. None only for the root (import) node.
    pub parent_id: Option<usize>,
    /// Child node IDs. Multiple children = branching.
    pub children_ids: Vec<usize>,
    /// The operation applied at this node. None for root.
    pub operation: Option<Operation>,
    /// Inverse data for undo. None for root.
    pub inverse: Option<InverseData>,
    /// When this operation was applied.
    pub applied_at: DateTime<Utc>,
    /// Soft-delete marker. Deleted nodes are kept but hidden from display.
    #[serde(default)]
    pub deleted: bool,
    /// Tag scope active when this operation was applied.
    #[serde(default)]
    pub tag_scope: Option<TagScopeRef>,
}

/// The full operation history tree stored in operations.json.
///
/// Replaces the flat `Vec<OperationRecord>` with a git-like DAG structure
/// supporting branching and non-destructive history navigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryTree {
    /// All nodes in the tree. Index 0 is always the root (import).
    pub nodes: Vec<HistoryNode>,
    /// Named branch pointers. Maps branch name → node_id it points to.
    /// "main" always exists.
    pub branches: HashMap<String, usize>,
    /// The currently active branch name.
    pub current_branch: String,
}

/// Serializable format for operations.json (tree version).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TreeFileFormat {
    pub nodes: Vec<HistoryNode>,
    pub branches: HashMap<String, usize>,
    pub current_branch: String,
}

/// Old flat format for backward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FlatOperationRecord {
    pub id: usize,
    pub operation: Operation,
    pub inverse: InverseData,
    pub applied_at: DateTime<Utc>,
}

impl HistoryTree {
    /// Create a new empty history tree with just a root node (import).
    pub fn new() -> Self {
        let root = HistoryNode {
            id: 0,
            parent_id: None,
            children_ids: Vec::new(),
            operation: None,
            inverse: None,
            applied_at: Utc::now(),
            deleted: false,
            tag_scope: None,
        };

        let mut branches = HashMap::new();
        branches.insert("main".to_string(), 0);

        Self {
            nodes: vec![root],
            branches,
            current_branch: "main".to_string(),
        }
    }

    /// Get the HEAD node ID for the current branch.
    pub fn head(&self) -> usize {
        self.branches
            .get(&self.current_branch)
            .copied()
            .unwrap_or(0)
    }

    /// Get the HEAD node ID for a specific branch.
    pub fn branch_head(&self, name: &str) -> Option<usize> {
        self.branches.get(name).copied()
    }

    /// Get the path (ancestor chain) from root to a given node.
    /// Returns node IDs from root to target (inclusive).
    pub fn path_to(&self, target_id: usize) -> Vec<usize> {
        let mut path = Vec::new();
        let mut current = Some(target_id);
        while let Some(id) = current {
            path.push(id);
            current = self.get_node(id).and_then(|n| n.parent_id);
        }
        path.reverse();
        path
    }

    /// Find the lowest common ancestor of multiple nodes.
    /// Returns `None` if any node is not found, otherwise `Some(lca_id)`.
    /// For a single node, returns the node itself. Returns root (0) if
    /// the only common ancestor is the root.
    pub fn lowest_common_ancestor(&self, node_ids: &[usize]) -> Option<usize> {
        if node_ids.is_empty() {
            return None;
        }
        let paths: Vec<Vec<usize>> = node_ids
            .iter()
            .map(|&id| self.path_to(id))
            .collect();
        // Find longest common prefix
        let min_len = paths.iter().map(|p| p.len()).min().unwrap_or(0);
        let mut lca = 0usize;
        for i in 0..min_len {
            let candidate = paths[0][i];
            if paths.iter().all(|p| p[i] == candidate) {
                lca = candidate;
            } else {
                break;
            }
        }
        Some(lca)
    }

    /// Get a reference to a node by ID.
    pub fn get_node(&self, id: usize) -> Option<&HistoryNode> {
        self.nodes.get(id)
    }

    /// Get a mutable reference to a node by ID.
    pub fn get_node_mut(&mut self, id: usize) -> Option<&mut HistoryNode> {
        self.nodes.get_mut(id)
    }

    /// Add a new child node to a parent. Returns the new node ID.
    pub fn add_child(
        &mut self,
        parent_id: usize,
        operation: Operation,
        inverse: InverseData,
    ) -> usize {
        self.add_child_with_scope(parent_id, operation, inverse, None)
    }

    /// Add a new child node with an optional tag scope. Returns the new node ID.
    pub fn add_child_with_scope(
        &mut self,
        parent_id: usize,
        operation: Operation,
        inverse: InverseData,
        tag_scope: Option<TagScopeRef>,
    ) -> usize {
        let new_id = self.nodes.len();
        let node = HistoryNode {
            id: new_id,
            parent_id: Some(parent_id),
            children_ids: Vec::new(),
            operation: Some(operation),
            inverse: Some(inverse),
            applied_at: Utc::now(),
            deleted: false,
            tag_scope,
        };

        // Register as child of parent
        if let Some(parent) = self.get_node_mut(parent_id) {
            parent.children_ids.push(new_id);
        }

        self.nodes.push(node);
        new_id
    }

    /// Move a branch HEAD to a new node. The node must exist.
    /// This is how operations are "applied" — the branch advances.
    pub fn advance_branch(&mut self, branch_name: &str, node_id: usize) -> bool {
        if self.get_node(node_id).is_some() {
            self.branches.insert(branch_name.to_string(), node_id);
            true
        } else {
            false
        }
    }

    /// Move current branch HEAD backward by one step (undo).
    /// Returns the operation that was undone.
    pub fn undo(&mut self) -> Option<&Operation> {
        let head_id = self.head();
        let node = self.get_node(head_id)?;

        // Can't undo past root
        let parent_id = node.parent_id?;

        // Move branch back to parent
        self.branches
            .insert(self.current_branch.clone(), parent_id);

        // Return the operation that was undone
        self.get_node(head_id)
            .and_then(|n| n.operation.as_ref())
    }

    /// Create a new branch pointing to a given node.
    /// Returns false if branch name already exists.
    pub fn create_branch(&mut self, name: &str, at_node_id: usize) -> bool {
        if self.branches.contains_key(name) || self.get_node(at_node_id).is_none() {
            return false;
        }
        self.branches.insert(name.to_string(), at_node_id);
        true
    }

    /// Delete a branch (cannot delete "main" or the current branch).
    pub fn delete_branch(&mut self, name: &str) -> bool {
        if name == "main" || name == self.current_branch {
            return false;
        }
        self.branches.remove(name).is_some()
    }

    /// Switch current branch. Returns false if branch doesn't exist.
    pub fn checkout_branch(&mut self, name: &str) -> bool {
        if self.branches.contains_key(name) {
            self.current_branch = name.to_string();
            true
        } else {
            false
        }
    }

    /// List all branch names.
    pub fn branch_names(&self) -> Vec<&str> {
        self.branches.keys().map(|s| s.as_str()).collect()
    }

    /// Total number of nodes (including root).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if tree is empty (just root, no operations or all deleted).
    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }

    // ── Node manipulation ──

    /// Soft-delete a node: mark it `deleted` and move any branches
    /// pointing to this node to its parent. The node remains in the
    /// tree so it can still be displayed (dimmed) and its patterns
    /// remain accessible via search history.
    ///
    /// The root node (id=0) cannot be deleted.
    /// Returns an error message if the node doesn't exist or is root.
    pub fn soft_delete(&mut self, node_id: usize) -> Result<(), String> {
        if node_id == 0 {
            return Err("Cannot delete the root node".into());
        }

        let parent_id = self
            .get_node(node_id)
            .ok_or_else(|| format!("Node {} not found", node_id))?
            .parent_id;

        // Mark the node as deleted
        if let Some(node) = self.get_node_mut(node_id) {
            node.deleted = true;
        }

        // Move branches pointing to this node to parent
        if let Some(pid) = parent_id {
            for (_, branch_head) in self.branches.iter_mut() {
                if *branch_head == node_id {
                    *branch_head = pid;
                }
            }
        }

        Ok(())
    }

    /// Check if `ancestor_id` is an ancestor of `descendant_id`.
    pub fn is_ancestor(&self, ancestor_id: usize, descendant_id: usize) -> bool {
        let path = self.path_to(descendant_id);
        path.contains(&ancestor_id)
    }

    /// Get all descendant node IDs of a given node (including itself).
    pub fn descendants(&self, node_id: usize) -> Vec<usize> {
        let mut result = Vec::new();
        self.collect_descendants(node_id, &mut result);
        result
    }

    fn collect_descendants(&self, node_id: usize, result: &mut Vec<usize>) {
        if self.get_node(node_id).is_none() {
            return;
        }
        result.push(node_id);
        if let Some(node) = self.get_node(node_id) {
            for &child_id in &node.children_ids {
                self.collect_descendants(child_id, result);
            }
        }
    }

    // ── Serialization ──

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let file = TreeFileFormat {
            nodes: self.nodes.clone(),
            branches: self.branches.clone(),
            current_branch: self.current_branch.clone(),
        };
        serde_json::to_string_pretty(&file)
    }

    /// Deserialize from JSON string. Auto-detects old flat format and migrates.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        // Try tree format first
        if let Ok(file) = serde_json::from_str::<TreeFileFormat>(json) {
            return Ok(Self {
                nodes: file.nodes,
                branches: file.branches,
                current_branch: file.current_branch,
            });
        }

        // Try old flat format and migrate
        if let Ok(records) = serde_json::from_str::<Vec<FlatOperationRecord>>(json) {
            return Ok(Self::migrate_from_flat(&records));
        }

        // Empty array (fresh repo)
        if let Ok(records) = serde_json::from_str::<Vec<FlatOperationRecord>>(json) {
            return Ok(Self::migrate_from_flat(&records));
        }

        // Fallback: return new empty tree
        Ok(Self::new())
    }

    /// Migrate from old flat Vec<OperationRecord> format to tree format.
    fn migrate_from_flat(records: &[FlatOperationRecord]) -> Self {
        let root = HistoryNode {
            id: 0,
            parent_id: None,
            children_ids: Vec::new(), // filled by loop below
            operation: None,
            inverse: None,
            applied_at: Utc::now(),
            deleted: false,
            tag_scope: None,
        };

        let mut nodes = vec![root];

        for record in records {
            let id = record.id + 1; // offset by 1 for root
            let parent_id = if record.id == 0 {
                Some(0)
            } else {
                Some(record.id) // previous record's node id
            };

            // Update parent's children
            if let Some(parent) = nodes.get_mut(parent_id.unwrap()) {
                parent.children_ids.push(id);
            }

            let next_id = id + 1;
            let children_ids = if record.id + 1 < records.len() {
                vec![next_id]
            } else {
                vec![]
            };

            nodes.push(HistoryNode {
                id,
                parent_id,
                children_ids,
                operation: Some(record.operation.clone()),
                inverse: Some(record.inverse.clone()),
                applied_at: record.applied_at,
                deleted: false,
                tag_scope: None,
            });
        }

        let head_id = if records.is_empty() {
            0
        } else {
            records.len() // last record id + 1
        };

        let mut branches = HashMap::new();
        branches.insert("main".to_string(), head_id);

        Self {
            nodes,
            branches,
            current_branch: "main".to_string(),
        }
    }

    // ── Tree traversal for display ──

    /// Collect all nodes in topological order (root first, then children in insertion order).
    /// Returns a list suitable for git-like display rendering.
    pub fn topological_order(&self) -> Vec<TopoEntry> {
        let mut result = Vec::new();
        // Root node: no parent, so is_last_child=true, sibling_count=1
        self.visit_subtree(0, 0, &mut Vec::new(), &mut result, true, 1);
        result
    }

    fn visit_subtree(
        &self,
        node_id: usize,
        depth: usize,
        continuing_forks: &mut Vec<bool>,
        result: &mut Vec<TopoEntry>,
        is_last_child: bool,
        sibling_count: usize,
    ) {
        let node = match self.get_node(node_id) {
            Some(n) => n,
            None => return,
        };

        let desc = match &node.operation {
            Some(op) => op.describe(),
            None => {
                // Root node: try to count original lines from metadata
                "Import".to_string()
            }
        };

        // Collect branch labels for this node
        let branch_labels: Vec<String> = self
            .branches
            .iter()
            .filter(|(_, &nid)| nid == node_id)
            .map(|(name, _)| name.clone())
            .collect();

        let is_current_head = self.head() == node_id;
        let num_children = node.children_ids.len();

        result.push(TopoEntry {
            node_id,
            depth,
            ancestors: continuing_forks.clone(),
            branch_labels,
            is_current_head,
            description: desc,
            applied_at: node.applied_at,
            has_children: num_children > 0,
            deleted: node.deleted,
            tag_name: node.tag_scope.as_ref().map(|s| s.tag_name.clone()),
            is_last_child,
            sibling_count,
        });

        // Visit children (include deleted nodes — they're shown dimmed).
        // Depth only increases at fork points (nodes with >1 child).
        // Linear chains (only-child nodes) stay at the same display depth.
        let is_fork = num_children > 1;
        let child_depth = if is_fork { depth + 1 } else { depth };

        for (i, &child_id) in node.children_ids.iter().enumerate() {
            let is_last = i == num_children - 1;
            if is_fork {
                // This node is a fork point: push whether this fork has more children coming.
                continuing_forks.push(!is_last);
            }
            self.visit_subtree(child_id, child_depth, continuing_forks, result, is_last, num_children);
            if is_fork {
                continuing_forks.pop();
            }
        }
    }
}

impl Default for HistoryTree {
    fn default() -> Self {
        Self::new()
    }
}

/// An entry in the topological ordering for display purposes.
#[derive(Debug, Clone)]
pub struct TopoEntry {
    pub node_id: usize,
    /// Display depth: only increments at fork points (nodes with >1 child).
    /// Linear chains share the same depth.
    pub depth: usize,
    /// At each display-depth level 0..depth-1, whether the fork at that level
    /// still has more children to show (controls vertical `│` continuation lines).
    pub ancestors: Vec<bool>,
    pub branch_labels: Vec<String>,
    pub is_current_head: bool,
    pub description: String,
    pub applied_at: DateTime<Utc>,
    pub has_children: bool,
    /// Whether this node is soft-deleted.
    pub deleted: bool,
    /// Tag scope name if this node was created with a tag scope.
    pub tag_name: Option<String>,
    /// Whether this node is the last child of its parent.
    pub is_last_child: bool,
    /// Number of children the parent has (1 = only child, >1 = fork point).
    pub sibling_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::{InverseData, Operation};

    fn make_filter_op() -> (Operation, InverseData) {
        let op = Operation::Filter {
            pattern: "ERROR".to_string(),
            keep: true,
        };
        let inv = InverseData::FilterInverse { removed: vec![] };
        (op, inv)
    }

    #[test]
    fn test_new_tree() {
        let tree = HistoryTree::new();
        assert_eq!(tree.nodes.len(), 1); // root
        assert_eq!(tree.head(), 0);
        assert_eq!(tree.current_branch, "main");
        assert_eq!(tree.branch_names(), vec!["main"]);
    }

    #[test]
    fn test_add_child() {
        let mut tree = HistoryTree::new();
        let (op, inv) = make_filter_op();
        let child_id = tree.add_child(0, op, inv);
        assert_eq!(child_id, 1);
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[0].children_ids, vec![1]);
        assert_eq!(tree.nodes[1].parent_id, Some(0));
    }

    #[test]
    fn test_advance_and_undo() {
        let mut tree = HistoryTree::new();
        let (op, inv) = make_filter_op();
        let child_id = tree.add_child(0, op, inv);
        tree.advance_branch("main", child_id);

        assert_eq!(tree.head(), 1);

        let undone = tree.undo();
        assert!(undone.is_some());
        assert_eq!(tree.head(), 0);
    }

    #[test]
    fn test_branching() {
        let mut tree = HistoryTree::new();
        let (op1, inv1) = make_filter_op();
        let child1 = tree.add_child(0, op1, inv1);
        tree.advance_branch("main", child1);

        // Create a branch at root
        assert!(tree.create_branch("experiment", 0));
        tree.checkout_branch("experiment");

        // Add a different operation on experiment
        let op2 = Operation::Replace {
            pattern: "foo".to_string(),
            replacement: "bar".to_string(),
        };
        let inv2 = InverseData::ReplaceInverse { originals: vec![] };
        let child2 = tree.add_child(0, op2, inv2);
        tree.advance_branch("experiment", child2);

        // Now root has two children (branching!)
        assert_eq!(tree.nodes[0].children_ids.len(), 2);
        assert_eq!(tree.nodes[0].children_ids, vec![1, 2]);
        assert_eq!(tree.head(), 2); // experiment HEAD

        // main still points to child1
        assert_eq!(tree.branch_head("main"), Some(1));
        assert_eq!(tree.branch_head("experiment"), Some(2));
    }

    #[test]
    fn test_migrate_flat_format() {
        use chrono::Utc;
        let records = vec![
            FlatOperationRecord {
                id: 0,
                operation: Operation::Filter {
                    pattern: "ERR".to_string(),
                    keep: true,
                },
                inverse: InverseData::FilterInverse { removed: vec![] },
                applied_at: Utc::now(),
            },
        ];
        let tree = HistoryTree::migrate_from_flat(&records);
        assert_eq!(tree.nodes.len(), 2); // root + 1 op
        assert_eq!(tree.nodes[0].children_ids, vec![1]);
        assert_eq!(tree.nodes[1].parent_id, Some(0));
        assert_eq!(tree.head(), 1);
    }

    #[test]
    fn test_migrate_empty() {
        let records: Vec<FlatOperationRecord> = vec![];
        let tree = HistoryTree::migrate_from_flat(&records);
        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.head(), 0);
    }

    #[test]
    fn test_topological_order() {
        let mut tree = HistoryTree::new();
        let (op1, inv1) = make_filter_op();
        let id1 = tree.add_child(0, op1, inv1);
        tree.advance_branch("main", id1);

        let (op2, inv2) = make_filter_op();
        let id2 = tree.add_child(id1, op2, inv2);
        tree.advance_branch("main", id2);

        let order = tree.topological_order();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0].node_id, 0);
        assert_eq!(order[1].node_id, 1);
        assert_eq!(order[2].node_id, 2);
        assert!(order[2].is_current_head);
    }

    #[test]
    fn test_path_to() {
        let mut tree = HistoryTree::new();
        let (op1, inv1) = make_filter_op();
        let id1 = tree.add_child(0, op1, inv1);
        tree.advance_branch("main", id1);

        let (op2, inv2) = make_filter_op();
        let id2 = tree.add_child(id1, op2, inv2);
        tree.advance_branch("main", id2);

        let path = tree.path_to(id2);
        assert_eq!(path, vec![0, 1, 2]);

        let path = tree.path_to(0);
        assert_eq!(path, vec![0]);
    }

    #[test]
    fn test_soft_delete_marks_deleted() {
        let mut tree = HistoryTree::new();
        let (op1, inv1) = make_filter_op();
        let id1 = tree.add_child(0, op1, inv1);

        tree.soft_delete(id1).unwrap();

        assert!(tree.get_node(id1).unwrap().deleted);
        // Branch should be moved to parent (root)
        assert_eq!(tree.head(), 0);
    }

    #[test]
    fn test_soft_delete_no_children_reparent() {
        let mut tree = HistoryTree::new();
        let (op1, inv1) = make_filter_op();
        let id1 = tree.add_child(0, op1, inv1.clone());
        tree.advance_branch("main", id1);

        let (op2, inv2) = make_filter_op();
        let id2 = tree.add_child(id1, op2, inv2);
        tree.advance_branch("main", id2);

        // Soft delete node 1 — children stay connected
        tree.soft_delete(1).unwrap();

        assert!(tree.get_node(1).unwrap().deleted);
        // Node 2's parent should still be 1 (tree structure preserved)
        assert_eq!(tree.get_node(2).unwrap().parent_id, Some(1));
        // Root should still have node 1 as child
        assert!(tree.nodes[0].children_ids.contains(&1));
        // Branch still points to 2 (wasn't pointing to deleted node)
        assert_eq!(tree.head(), 2);
    }

    #[test]
    fn test_soft_delete_root_fails() {
        let mut tree = HistoryTree::new();
        assert!(tree.soft_delete(0).is_err());
    }

    #[test]
    fn test_is_ancestor() {
        let mut tree = HistoryTree::new();
        let (op, inv) = make_filter_op();
        let id1 = tree.add_child(0, op, inv);
        let (op2, inv2) = make_filter_op();
        let id2 = tree.add_child(id1, op2, inv2);

        assert!(tree.is_ancestor(0, id2));
        assert!(tree.is_ancestor(id1, id2));
        assert!(!tree.is_ancestor(id2, id1));
        assert!(tree.is_ancestor(0, 0));
    }

    #[test]
    fn test_descendants() {
        let mut tree = HistoryTree::new();
        let (op, inv) = make_filter_op();
        let id1 = tree.add_child(0, op, inv);
        let (op2, inv2) = make_filter_op();
        let id2 = tree.add_child(id1, op2, inv2);

        let desc = tree.descendants(0);
        assert_eq!(desc.len(), 3);
        assert!(desc.contains(&0));
        assert!(desc.contains(&1));
        assert!(desc.contains(&2));

        let desc = tree.descendants(1);
        assert_eq!(desc.len(), 2);
        assert!(desc.contains(&1));
        assert!(desc.contains(&2));
    }

    #[test]
    fn test_add_child_with_scope() {
        use crate::tag::TagScopeRef;
        let mut tree = HistoryTree::new();
        let (op, inv) = make_filter_op();
        let scope = TagScopeRef {
            tag_name: "errors".into(),
            ranges: vec![(0, 50)],
        };
        let id = tree.add_child_with_scope(0, op, inv, Some(scope));
        let node = tree.get_node(id).unwrap();
        assert!(node.tag_scope.is_some());
        assert_eq!(node.tag_scope.as_ref().unwrap().tag_name, "errors");
    }

    #[test]
    fn test_topological_order_includes_deleted() {
        let mut tree = HistoryTree::new();
        let (op, inv) = make_filter_op();
        let id1 = tree.add_child(0, op, inv);
        tree.advance_branch("main", id1);

        // Add a second child so node 1 still has a child in the tree
        let (op2, inv2) = make_filter_op();
        let id2 = tree.add_child(id1, op2, inv2);
        tree.advance_branch("main", id2);

        tree.soft_delete(id1).unwrap();

        let order = tree.topological_order();
        // Root, deleted node, and child should all appear
        assert_eq!(order.len(), 3);
        assert!(order[1].deleted);
        assert_eq!(order[1].node_id, id1);
    }
}
