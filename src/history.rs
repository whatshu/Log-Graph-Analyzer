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

    /// Soft-delete a node: mark it `deleted` and reparent its children
    /// to the deleted node's parent. Branches pointing to the deleted node
    /// are moved to its parent.
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

        let children: Vec<usize> = self
            .get_node(node_id)
            .map(|n| n.children_ids.clone())
            .unwrap_or_default();

        // Mark the node as deleted
        if let Some(node) = self.get_node_mut(node_id) {
            node.deleted = true;
        }

        // Reparent all children to the deleted node's parent
        if let Some(pid) = parent_id {
            for &child_id in &children {
                if let Some(child) = self.get_node_mut(child_id) {
                    child.parent_id = Some(pid);
                }
            }
            // Add children to parent's children list
            if let Some(parent) = self.get_node_mut(pid) {
                parent.children_ids.retain(|&id| id != node_id);
                parent.children_ids.extend(&children);
            }
        } else {
            // Node had no parent (shouldn't happen for non-root, but handle gracefully)
            for &child_id in &children {
                if let Some(child) = self.get_node_mut(child_id) {
                    child.parent_id = None;
                }
            }
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
        self.visit_subtree(0, 0, &mut Vec::new(), &mut result);
        result
    }

    fn visit_subtree(
        &self,
        node_id: usize,
        depth: usize,
        ancestors: &mut Vec<usize>,
        result: &mut Vec<TopoEntry>,
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

        result.push(TopoEntry {
            node_id,
            depth,
            ancestors: ancestors.clone(),
            branch_labels,
            is_current_head,
            description: desc,
            applied_at: node.applied_at,
            has_children: !node.children_ids.is_empty(),
            deleted: node.deleted,
            tag_name: node.tag_scope.as_ref().map(|s| s.tag_name.clone()),
        });

        // Visit children (include deleted nodes — they're shown dimmed)
        for (i, &child_id) in node.children_ids.iter().enumerate() {
            let is_last = i == node.children_ids.len() - 1;
            if !is_last {
                ancestors.push(node_id);
            }
            self.visit_subtree(child_id, depth + 1, ancestors, result);
            if !is_last {
                ancestors.pop();
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
    pub depth: usize,
    pub ancestors: Vec<usize>,
    pub branch_labels: Vec<String>,
    pub is_current_head: bool,
    pub description: String,
    pub applied_at: DateTime<Utc>,
    pub has_children: bool,
    /// Whether this node is soft-deleted.
    pub deleted: bool,
    /// Tag scope name if this node was created with a tag scope.
    pub tag_name: Option<String>,
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
}
