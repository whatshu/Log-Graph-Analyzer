//! Core state types for the TUI application.
//!
//! This module contains the type definitions (enums, structs) that form
//! the application state model. Business logic lives in [`app`](super::app).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use lograph::cache::CacheManager;
use lograph::config::Config;
use lograph::operator::{MergeMode, Operation};
use lograph::repo::{LogRepo, Workspace};
use lograph::tag::TagStore;

use super::file_browser::FileBrowser;

// ── View & Input Enums ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ViewKind {
    LogView,
    RepoList,
    Analytics,
    History,
    FileBrowser,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Command,
    Search,
    Input,
    FileBrowser,
}

// ── Deferred Operations ──

#[derive(Debug)]
pub enum PendingOp {
    None,
    OpenRepo(String),
    ApplyOperation(Operation),
    /// Apply operation from a specific node (used for branching off)
    ApplyOperationFrom(usize, Operation),
    Undo,
    /// Checkout (view) a history node non-destructively
    CheckoutTo(usize),
    ExportFrom(usize, String),
    /// Merge marked nodes
    MergeNodes {
        sources: Vec<usize>,
        branch: String,
        mode: MergeMode,
    },
    /// Subtract one node from another
    SubtractNodes {
        base: usize,
        subtrahend: usize,
        branch: String,
    },
    /// Replay a node's operation at a different position
    ReplayNode {
        source: usize,
        target_parent: usize,
        branch: String,
    },
    /// Soft-delete a node
    SoftDelete { node_id: usize },
}

// ── Application State ──

pub struct App {
    pub config: Config,
    pub workspace: Workspace,
    pub repo: RefCell<Option<LogRepo>>,
    pub repo_name: String,
    pub active_view: ViewKind,
    pub scroll_offset: usize,
    pub horizontal_scroll: usize,
    pub cursor_line: usize,
    pub viewport_lines: Vec<String>,
    pub total_lines: usize,
    pub line_count_is_original: bool,
    pub search_query: String,
    pub search_results: Vec<usize>,
    pub search_index: usize,
    /// Search/filter history (most recent first, max 100).
    pub search_history: Vec<String>,
    /// Current position in history navigation (-1 = not navigating).
    pub search_history_idx: isize,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub input_prompt: String,
    pub status_message: String,
    pub error_message: Option<String>,
    pub show_help: bool,
    pub should_quit: bool,

    // Repo list view
    pub repo_cursor: usize,

    // History view
    pub history_cursor: usize,
    pub history_nodes: Vec<HistoryNode>,
    /// Node currently being viewed (None = viewing current branch HEAD).
    pub viewed_node_id: Option<usize>,
    /// Whether HEAD is detached (not on any branch).
    pub detached_head: bool,

    // History node operations
    /// Marked node IDs for multi-select merge
    pub history_marks: HashSet<usize>,
    /// Yanked node ID for copy/paste
    pub yanked_node_id: Option<usize>,
    /// Diff mode: first selected node (base)
    pub diff_base_node_id: Option<usize>,

    // File browser
    pub file_browser: FileBrowser,

    // Caching
    pub cache_manager: CacheManager,

    // Tmux
    pub in_tmux: bool,

    // Terminal dimensions (updated each render)
    pub terminal_width: u16,

    // Use ASCII-only characters (no Unicode box-drawing or emoji).
    // Auto-detected from locale; also useful for terminals that don't
    // support UTF-8.
    pub ascii_only: bool,

    // Pending history export (node_idx, export started flag)
    pub pending_history_export: Option<usize>,
    /// Pending repo clone source name
    pub pending_repo_clone_src: Option<String>,

    // Collect results stored by node_id
    pub collect_results: HashMap<usize, String>,
    /// Pending collect summary — set by run_collect, consumed by apply_pending.
    pub pending_collect_summary: Option<String>,
    pub collect_detail: Option<String>,
    pub show_collect_detail: bool,

    // ── Tag system ──

    /// Tag store (loaded from disk)
    pub tag_store: TagStore,
    /// Tag manager popup
    pub show_tag_manager: bool,
    pub tag_manager_cursor: usize,
    pub tag_manager_scroll: usize,
    pub tag_manager_h_scroll: usize,
    pub pending_tag_rename: Option<String>,

    /// Merge mode selection popup
    pub show_merge_mode_popup: bool,
    pub merge_mode_cursor: usize,
    pub merge_sources: Vec<usize>,

    // ── Search tag range ──
    pub search_tag_start: Option<String>,
    pub search_tag_end: Option<String>,
    pub picking_search_tag: bool,
    pub search_tag_pick_cursor: usize,

    pub pending_op: PendingOp,
}

// ── History View Model ──

#[derive(Clone)]
pub struct HistoryNode {
    pub id: usize,
    pub description: String,
    pub line_count: usize,
    pub applied_at: String,
    /// Tree connector prefix: "│  ", "├─ ", "└─ ", "   "
    pub connector: String,
    /// Depth in the tree (for indentation).
    #[allow(dead_code)]
    pub depth: usize,
    /// Branch labels at this node.
    pub branch_labels: Vec<String>,
    /// Is this the current branch HEAD?
    pub is_head: bool,
    /// Is this node being viewed?
    pub is_viewed: bool,
    /// Collect result summary if a collect was run at this node.
    pub collect_summary: Option<String>,
    /// Whether this node is soft-deleted.
    pub deleted: bool,
    /// Tag scope name if this node was created with a tag scope.
    #[allow(dead_code)]
    pub tag_name: Option<String>,
    /// Whether this node is the last child of its parent.
    pub is_last_child: bool,
    /// Number of siblings (children of parent). 1 = only child, >1 = fork.
    pub sibling_count: usize,
}

// ── Utility Functions ──

/// Detect whether the terminal locale supports UTF-8 rendering.
/// Checks `LC_ALL`, `LC_CTYPE`, and `LANG` in order. Returns `true`
/// if any of them ends with `.UTF-8` or `.utf8` (case-insensitive).
pub fn detect_utf8_locale() -> bool {
    for var in &["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            let upper = val.to_uppercase();
            if upper.ends_with("UTF-8") || upper.ends_with("UTF8") {
                return true;
            }
        }
    }
    false
}

/// Set tmux/xterm window title via ANSI escape sequences.
pub fn set_tmux_title(title: &str) {
    // Set both window title and icon name
    print!("\x1b]0;{}\x07", title);
    // Also set xterm window title
    print!("\x1b]2;{}\x07", title);
}
