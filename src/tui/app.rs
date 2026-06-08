use std::cell::RefCell;
use std::path::Path;

use log_analyzer_core::cache::CacheManager;
use log_analyzer_core::config::Config;
use log_analyzer_core::error::Result;
use log_analyzer_core::operator::Operation;
use log_analyzer_core::repo::{LogRepo, Workspace};

use std::path::PathBuf;

use super::file_browser::FileBrowser;

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

enum PendingOp {
    None,
    OpenRepo(String),
    ApplyOperation(Operation),
    /// Apply operation from a specific node (used for branching off)
    ApplyOperationFrom(usize, Operation),
    Undo,
    /// Checkout (view) a history node non-destructively
    CheckoutTo(usize),
    ExportFrom(usize, String),
}

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

    // History view
    pub history_cursor: usize,
    pub history_nodes: Vec<HistoryNode>,
    /// Node currently being viewed (None = viewing current branch HEAD).
    pub viewed_node_id: Option<usize>,
    /// Whether HEAD is detached (not on any branch).
    pub detached_head: bool,

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

    pending_op: PendingOp,
}

#[derive(Clone)]
pub struct HistoryNode {
    pub id: usize,
    pub description: String,
    pub line_count: usize,
    pub applied_at: String,
    /// Tree connector prefix: "│  ", "├─ ", "└─ ", "   "
    pub connector: String,
    /// Depth in the tree (for indentation).
    pub depth: usize,
    /// Branch labels at this node.
    pub branch_labels: Vec<String>,
    /// Is this the current branch HEAD?
    pub is_head: bool,
    /// Is this node being viewed?
    pub is_viewed: bool,
}

/// Detect whether the terminal locale supports UTF-8 rendering.
/// Checks `LC_ALL`, `LC_CTYPE`, and `LANG` in order. Returns `true`
/// if any of them ends with `.UTF-8` or `.utf8` (case-insensitive).
fn detect_utf8_locale() -> bool {
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

impl App {
    pub fn new(workspace_root: &Path, initial_repo: Option<&str>) -> Result<Self> {
        let config = Config::load();
        let workspace = Workspace::open(workspace_root)?;
        let _ = workspace.migrate_if_needed();

        let in_tmux = std::env::var("TMUX").is_ok();

        let cache_dir = PathBuf::from(".log_analyzer").join("cache");
        let cache_manager = CacheManager::new(cache_dir, config.cache.clone())
            .unwrap_or_else(|_| {
                // Fallback: in-memory only (directory inaccessible)
                CacheManager::new(
                    PathBuf::from(".log_analyzer").join("cache"),
                    log_analyzer_core::cache::CacheConfig::default(),
                )
                .unwrap_or_else(|_| {
                    // This shouldn't fail with default config since we create dirs
                    panic!("Failed to create cache manager")
                })
            });

        let mut app = Self {
            config,
            workspace,
            cache_manager,
            repo: RefCell::new(None),
            repo_name: String::new(),
            active_view: ViewKind::LogView,
            scroll_offset: 0,
            horizontal_scroll: 0,
            cursor_line: 0,
            viewport_lines: Vec::new(),
            total_lines: 0,
            line_count_is_original: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_index: 0,
            search_history: Self::load_search_history(),
            search_history_idx: -1,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            input_prompt: String::new(),
            status_message: String::new(),
            error_message: None,
            show_help: false,
            should_quit: false,
            history_cursor: 0,
            history_nodes: Vec::new(),
            viewed_node_id: None,
            detached_head: false,
            file_browser: FileBrowser::new(Path::new(".")),
            in_tmux,
            terminal_width: 80, // default, updated on first render
            ascii_only: !detect_utf8_locale(),
            pending_history_export: None,
            pending_op: PendingOp::None,
        };

        let repo_to_open: Option<String> = match initial_repo {
            Some(name) => Some(name.to_string()),
            None => {
                if app.workspace.is_initialized() {
                    app.workspace.active().ok()
                } else {
                    None
                }
            }
        };

        if let Some(ref name) = repo_to_open {
            if app.workspace.has_repo(name) {
                app.do_open_repo(name);
            }
        } else {
            app.status_message =
                String::from("No repos found. Press 'i' to import.");
        }

        Ok(app)
    }

    fn do_open_repo(&mut self, name: &str) {
        match self.workspace.open_repo(name) {
            Ok(repo) => {
                self.total_lines = repo.original_line_count();
                self.line_count_is_original = repo.history_tree().is_empty();
                self.repo_name = name.to_string();
                *self.repo.borrow_mut() = Some(repo);
                self.scroll_offset = 0;
                self.cursor_line = 0;
                self.active_view = ViewKind::LogView;
                self.load_viewport();
                self.status_message = format!("Opened repo '{}'", name);
                let _ = self.workspace.set_active(name);
                if self.in_tmux {
                    set_tmux_title(&format!("log-analyzer: {}", name));
                }
            }
            Err(e) => {
                self.error_message =
                    Some(format!("Failed to open repo '{}': {}", name, e));
            }
        }
    }

    pub fn load_viewport(&mut self) {
        // If viewing a specific node, show its state (cached).
        if self.viewed_node_id.is_some() {
            let node_id = self.viewed_node_id.unwrap();
            let lines_result = self.get_node_lines(node_id).ok();
            if let Some(lines) = lines_result {
                self.total_lines = lines.len();
                self.line_count_is_original = node_id == 0;
                self.clamp_scroll_state();
                let start = self.scroll_offset.min(lines.len().saturating_sub(1));
                let end = (start + 200).min(lines.len());
                self.viewport_lines = lines[start..end].to_vec();
            } else {
                self.viewport_lines.clear();
            }
            return;
        }

        let repo_ref = self.repo.borrow();
        if repo_ref.is_none() {
            self.viewport_lines.clear();
            return;
        }

        let has_ops = repo_ref.as_ref().map_or(false, |r| !r.history_tree().is_empty());
        drop(repo_ref);

        // Get total_lines first, so we can clamp before reading the viewport
        if has_ops {
            let total = {
                let mut repo_mut = self.repo.borrow_mut();
                repo_mut
                    .as_mut()
                    .map(|r| r.current_line_count().unwrap_or(0))
                    .unwrap_or(0)
            };
            self.total_lines = total;
            self.clamp_scroll_state();

            let mut repo_mut = self.repo.borrow_mut();
            if let Some(ref mut r) = *repo_mut {
                self.viewport_lines = r
                    .read_current_lines(self.scroll_offset, 200)
                    .unwrap_or_default();
            }
        } else {
            self.total_lines = {
                let repo_ref = self.repo.borrow();
                repo_ref.as_ref().map(|r: &LogRepo| r.original_line_count()).unwrap_or(0)
            };
            self.clamp_scroll_state();

            let repo_ref = self.repo.borrow();
            if let Some(ref r) = *repo_ref {
                self.viewport_lines = r
                    .read_original_lines(self.scroll_offset, 200)
                    .unwrap_or_default();
            }
        }
        self.line_count_is_original = !has_ops;
    }

    /// Clamp scroll_offset and cursor_line to the current total_lines range.
    fn clamp_scroll_state(&mut self) {
        if self.total_lines > 0 {
            let max_offset = self.total_lines.saturating_sub(1);
            self.scroll_offset = self.scroll_offset.min(max_offset);
            self.cursor_line = self.cursor_line.min(max_offset);
        } else {
            self.scroll_offset = 0;
            self.cursor_line = 0;
        }
    }

    pub fn refresh_line_count(&mut self) {
        let repo_ref = self.repo.borrow();
        if let Some(ref r) = *repo_ref {
            if r.history_tree().is_empty() {
                self.total_lines = r.original_line_count();
                self.line_count_is_original = true;
            }
        }
        drop(repo_ref);

        if !self.line_count_is_original {
            let mut repo_mut = self.repo.borrow_mut();
            if let Some(ref mut r) = *repo_mut {
                self.total_lines = r.current_line_count().unwrap_or(0);
            }
        }
    }

    /// Build the history view nodes from the tree topological order.
    pub fn build_history(&mut self) {
        self.history_nodes.clear();
        let repo_ref = self.repo.borrow();
        if let Some(ref r) = *repo_ref {
            let tree = r.history_tree();
            let order = tree.topological_order();

            let head_id = tree.head();
            let viewed_id = self.viewed_node_id;

            for entry in &order {
                let desc = if entry.node_id == 0 {
                    format!("Import — {} lines", r.original_line_count())
                } else {
                    entry.description.clone()
                };
                let line_count = if entry.node_id == 0 {
                    r.original_line_count()
                } else {
                    r.line_count_at(entry.node_id).unwrap_or(0)
                };
                let is_head = entry.node_id == head_id || entry.is_current_head;
                let is_viewed = viewed_id == Some(entry.node_id);

                // Build tree connector
                let connector = build_connector(entry.depth, &entry.ancestors, entry.has_children, self.ascii_only);

                self.history_nodes.push(HistoryNode {
                    id: entry.node_id,
                    description: desc,
                    line_count,
                    applied_at: if entry.node_id == 0 {
                        String::from("—")
                    } else {
                        entry
                            .applied_at
                            .format("%Y-%m-%d %H:%M")
                            .to_string()
                    },
                    connector,
                    depth: entry.depth,
                    branch_labels: entry.branch_labels.clone(),
                    is_head,
                    is_viewed,
                });
            }
        }
        // Set cursor to HEAD node
        let head_id = {
            let repo_ref = self.repo.borrow();
            repo_ref
                .as_ref()
                .map(|r| r.head_node_id())
                .unwrap_or(0)
        };
        self.history_cursor = self
            .history_nodes
            .iter()
            .position(|n| n.id == head_id)
            .unwrap_or(self.history_nodes.len().saturating_sub(1));
    }

    pub fn open_repo(&mut self, name: Option<&str>) {
        let name = match name {
            Some(n) => n.to_string(),
            None => match self.workspace.active() {
                Ok(n) => n,
                Err(_) => return,
            },
        };
        if !self.workspace.has_repo(&name) {
            self.error_message = Some(format!("Repository '{}' not found", name));
            return;
        }
        self.pending_op = PendingOp::OpenRepo(name);
    }

    pub fn open_file_browser(&mut self) {
        let start_dir = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        self.file_browser = FileBrowser::new(&start_dir);
        self.input_mode = InputMode::FileBrowser;
    }

    pub fn import_from_file_browser(&mut self) {
        if let Some(path) = self.file_browser.selected_path() {
            let name = path
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| String::from("default"));
            match self.workspace.import_file(&name, &path) {
                Ok(repo) => {
                    self.total_lines = repo.original_line_count();
                    self.line_count_is_original = true;
                    self.repo_name = name.clone();
                    *self.repo.borrow_mut() = Some(repo);
                    self.scroll_offset = 0;
                    self.cursor_line = 0;
                    self.active_view = ViewKind::LogView;
                    self.input_mode = InputMode::Normal;
                    self.load_viewport();
                    self.status_message =
                        format!("Imported '{}' as '{}'", path.display(), name);
                }
                Err(e) => {
                    self.error_message = Some(format!("Import failed: {}", e));
                }
            }
        }
    }

    pub fn queue_operation(&mut self, op: Operation) {
        // If viewing a historical (non-HEAD) node, branch off instead
        if let (Some(node_id), true) = (self.viewed_node_id, self.detached_head) {
            self.pending_op = PendingOp::ApplyOperationFrom(node_id, op);
        } else {
            self.pending_op = PendingOp::ApplyOperation(op);
        }
    }

    pub fn queue_operation_from(&mut self, node_id: usize, op: Operation) {
        self.pending_op = PendingOp::ApplyOperationFrom(node_id, op);
    }

    pub fn queue_undo(&mut self) {
        self.pending_op = PendingOp::Undo;
    }

    pub fn queue_checkout(&mut self, node_idx: usize) {
        self.pending_op = PendingOp::CheckoutTo(node_idx);
    }

    /// Return to HEAD from viewed node mode.
    /// Get lines at a node, using cache if available.
    pub fn get_node_lines(&mut self, node_id: usize) -> Result<Vec<String>> {
        let repo_hash = {
            let repo_ref = self.repo.borrow();
            repo_ref
                .as_ref()
                .map(|r| log_analyzer_core::cache::hash_repo_path(r.path()))
                .unwrap_or_default()
        };

        // Try cache first
        if let Some(lines) = self.cache_manager.get(&repo_hash, node_id) {
            return Ok(lines);
        }

        // Compute and cache
        let lines = {
            let repo_ref = self.repo.borrow();
            if let Some(ref r) = *repo_ref {
                r.view_node(node_id)?
            } else {
                return Err(log_analyzer_core::error::LogAnalyzerError::Repo(
                    "No repo open".to_string(),
                ));
            }
        };

        // Cache the result
        let _ = self.cache_manager.put(&repo_hash, node_id, &lines);

        Ok(lines)
    }

    /// Return to HEAD from viewed node mode.
    pub fn return_to_head(&mut self) {
        self.viewed_node_id = None;
        self.detached_head = false;
        self.status_message = String::from("Returned to HEAD");
        self.load_viewport();
        self.re_search();
    }

    pub fn queue_export_from(&mut self, node_idx: usize, path: String) {
        self.pending_op = PendingOp::ExportFrom(node_idx, path);
    }

    pub fn apply_pending(&mut self) {
        let pending = std::mem::replace(&mut self.pending_op, PendingOp::None);
        match pending {
            PendingOp::None => {}
            PendingOp::OpenRepo(name) => self.do_open_repo(&name),
            PendingOp::ApplyOperation(op) => {
                let desc = op.describe();
                let mut repo_mut = self.repo.borrow_mut();
                if let Some(ref mut r) = *repo_mut {
                    match r.apply_operation(op) {
                        Ok(()) => {
                            self.status_message = format!("Applied: {}", desc);
                            self.line_count_is_original = false;
                        }
                        Err(e) => {
                            self.error_message =
                                Some(format!("Operation failed: {}", e));
                        }
                    }
                }
                drop(repo_mut);
                self.refresh_line_count();
                self.load_viewport();
                self.re_search();
            }
            PendingOp::Undo => {
                let mut repo_mut = self.repo.borrow_mut();
                if let Some(ref mut r) = *repo_mut {
                    match r.undo() {
                        Ok(op) => {
                            self.status_message =
                                format!("Undone: {}", op.describe());
                            if r.history_tree().is_empty() {
                                self.line_count_is_original = true;
                            }
                        }
                        Err(e) => {
                            self.error_message =
                                Some(format!("Undo failed: {}", e));
                        }
                    }
                }
                drop(repo_mut);
                self.refresh_line_count();
                self.load_viewport();
                self.re_search();
            }
            PendingOp::CheckoutTo(node_idx) => {
                let node_id = node_idx;
                // Check detached status before getting lines
                let is_detached = {
                    let repo_ref = self.repo.borrow();
                    repo_ref.as_ref().map_or(true, |r| node_id != r.head_node_id())
                };
                // Get lines via cache
                match self.get_node_lines(node_id) {
                    Ok(lines) => {
                        self.viewed_node_id = Some(node_id);
                        self.detached_head = is_detached;
                        self.status_message = format!(
                            "Viewing node {} ({}). Apply operation to branch off.",
                            node_id,
                            if is_detached { "detached" } else { "HEAD" }
                        );
                        self.total_lines = lines.len();
                        self.line_count_is_original = node_id == 0;
                    }
                    Err(e) => {
                        self.error_message =
                            Some(format!("View node failed: {}", e));
                        return;
                    }
                }
                self.load_viewport();
                self.re_search();
                self.build_history();
            }
            PendingOp::ExportFrom(node_idx, path) => {
                let repo_ref = self.repo.borrow();
                if let Some(ref r) = *repo_ref {
                    let node_id = node_idx;
                    let result = if node_id == 0 {
                        // Export original
                        r.read_all_original_lines()
                            .map(|lines| lines.join("\n"))
                    } else {
                        // Export at specific node
                        r.compute_state_at(node_id)
                            .map(|lines| lines.join("\n"))
                    };
                    match result {
                        Ok(content) => {
                            if let Err(e) = std::fs::write(&path, &content) {
                                self.error_message =
                                    Some(format!("Export failed: {}", e));
                            } else {
                                self.status_message =
                                    format!("Exported to {}", path);
                            }
                        }
                        Err(e) => {
                            self.error_message =
                                Some(format!("Export failed: {}", e));
                        }
                    }
                } else {
                    self.error_message = Some("No repo open".to_string());
                }
            }
            PendingOp::ApplyOperationFrom(from_node_id, op) => {
                let desc = op.describe();
                let mut repo_mut = self.repo.borrow_mut();
                if let Some(ref mut r) = *repo_mut {
                    let branch_name = match &op {
                        Operation::Filter { pattern, keep } => {
                            if *keep {
                                format!("filter-{}", pattern)
                            } else {
                                format!("filter-rm-{}", pattern)
                            }
                        }
                        Operation::Replace { pattern, .. } => {
                            format!("replace-{}", pattern)
                        }
                        _ => String::from("branch"),
                    };
                    match r.apply_operation_from(from_node_id, &branch_name, op) {
                        Ok(()) => {
                            self.status_message = format!(
                                "Created branch '{}' and applied: {}",
                                branch_name, desc
                            );
                            self.line_count_is_original = false;
                            self.viewed_node_id = None;
                            self.detached_head = false;
                        }
                        Err(e) => {
                            self.error_message =
                                Some(format!("Branch operation failed: {}", e));
                        }
                    }
                }
                drop(repo_mut);
                self.refresh_line_count();
                self.load_viewport();
                self.re_search();
            }
        }
    }

    pub fn do_search(&mut self, query: &str) {
        self.search_query = query.to_string();
        self.search_results.clear();
        self.search_index = 0;

        let results: Vec<usize> = {
            // If viewing a specific node, get its state
            if let Some(node_id) = self.viewed_node_id {
                let repo_ref = self.repo.borrow();
                if let Some(ref r) = *repo_ref {
                    let lines = r.view_node(node_id).unwrap_or_default();
                    match regex::Regex::new(query) {
                        Ok(re) => lines
                            .iter()
                            .enumerate()
                            .filter(|(_, line)| re.is_match(line))
                            .take(10_000)
                            .map(|(i, _)| i)
                            .collect(),
                        Err(_) => Vec::new(),
                    }
                } else {
                    Vec::new()
                }
            } else {
                let repo_ref = self.repo.borrow();
                if repo_ref.is_none() {
                    return;
                }

                let has_ops = repo_ref.as_ref().map_or(false, |r: &LogRepo| !r.history_tree().is_empty());
                if has_ops {
                    drop(repo_ref);
                    let mut repo_mut = self.repo.borrow_mut();
                    if let Some(ref mut r) = *repo_mut {
                        let lines = r.get_current_lines().unwrap_or_default();
                        match regex::Regex::new(query) {
                            Ok(re) => lines
                                .iter()
                                .enumerate()
                                .filter(|(_, line)| re.is_match(line))
                                .take(10_000)
                                .map(|(i, _)| i)
                                .collect(),
                            Err(_) => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    let r = repo_ref.as_ref().unwrap();
                    let proc = r.processor();
                    proc.parallel_search(query, 10_000)
                        .unwrap_or_default()
                        .iter()
                        .map(|(idx, _)| *idx)
                        .collect()
                }
            }
        };

        self.search_results = results;
        if !self.search_results.is_empty() {
            self.search_index = 0;
            let target = self.search_results[0];
            self.cursor_line = target;
            if target < self.scroll_offset || target >= self.scroll_offset + 50 {
                self.scroll_offset = target.saturating_sub(5);
            }
            self.load_viewport();
            self.status_message = format!(
                "Match {}/{}",
                self.search_index + 1,
                self.search_results.len()
            );
        } else {
            self.status_message = String::from("No matches found");
        }
    }

    /// Re-run the current search pattern against the current line space.
    /// Used after data-mutating operations (filter/replace/undo/checkout) to
    /// keep search highlights and n/N navigation accurate.
    /// Does NOT reload the viewport (caller must do that first).
    pub fn re_search(&mut self) {
        if self.search_query.is_empty() {
            return;
        }

        let query = self.search_query.clone();
        let results: Vec<usize> = {
            // If viewing a specific node, get its state
            if let Some(node_id) = self.viewed_node_id {
                let repo_ref = self.repo.borrow();
                if let Some(ref r) = *repo_ref {
                    let lines = r.view_node(node_id).unwrap_or_default();
                    match regex::Regex::new(&query) {
                        Ok(re) => lines
                            .iter()
                            .enumerate()
                            .filter(|(_, line)| re.is_match(line))
                            .take(10_000)
                            .map(|(i, _)| i)
                            .collect(),
                        Err(_) => Vec::new(),
                    }
                } else {
                    Vec::new()
                }
            } else {
                let repo_ref = self.repo.borrow();
                if repo_ref.is_none() {
                    return;
                }

                let has_ops =
                    repo_ref
                        .as_ref()
                        .map_or(false, |r: &LogRepo| !r.history_tree().is_empty());
                if has_ops {
                    drop(repo_ref);
                    let mut repo_mut = self.repo.borrow_mut();
                    if let Some(ref mut r) = *repo_mut {
                        let lines = r.get_current_lines().unwrap_or_default();
                        match regex::Regex::new(&query) {
                            Ok(re) => lines
                                .iter()
                                .enumerate()
                                .filter(|(_, line)| re.is_match(line))
                                .take(10_000)
                                .map(|(i, _)| i)
                                .collect(),
                            Err(_) => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    let r = repo_ref.as_ref().unwrap();
                    let proc = r.processor();
                    proc.parallel_search(&query, 10_000)
                        .unwrap_or_default()
                        .iter()
                        .map(|(idx, _)| *idx)
                        .collect()
                }
            }
        };

        self.search_results = results;
        self.search_index = 0;
    }

    pub fn next_match(&mut self) {
        if self.search_results.is_empty() { return; }
        self.search_index = (self.search_index + 1) % self.search_results.len();
        self.jump_to_match();
    }

    pub fn prev_match(&mut self) {
        if self.search_results.is_empty() { return; }
        self.search_index = if self.search_index == 0 {
            self.search_results.len() - 1
        } else {
            self.search_index - 1
        };
        self.jump_to_match();
    }

    fn jump_to_match(&mut self) {
        let target = self.search_results[self.search_index];
        self.cursor_line = target;
        if target < self.scroll_offset || target >= self.scroll_offset + 50 {
            self.scroll_offset = target.saturating_sub(5);
        }
        self.load_viewport();
        self.status_message = format!(
            "Match {}/{}",
            self.search_index + 1,
            self.search_results.len()
        );
    }

    pub fn scroll_down(&mut self, n: usize) {
        if self.total_lines == 0 { return; }
        let max_scroll = self.total_lines.saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + n).min(max_scroll);
        self.load_viewport();
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        self.load_viewport();
    }

    pub fn go_to_line(&mut self, line: usize) {
        let max_line = self.total_lines.saturating_sub(1);
        self.cursor_line = line.min(max_line);
        if self.cursor_line < self.scroll_offset || self.cursor_line >= self.scroll_offset + 50 {
            self.scroll_offset = self.cursor_line.saturating_sub(15);
        }
        self.load_viewport();
    }

    pub fn scroll_right(&mut self, n: usize) {
        self.horizontal_scroll = self.horizontal_scroll.saturating_add(n);
    }
    pub fn scroll_left(&mut self, n: usize) {
        self.horizontal_scroll = self.horizontal_scroll.saturating_sub(n);
    }
    /// Go to line start (horizontal 0), like vim `0`.
    pub fn go_to_line_start(&mut self) {
        self.horizontal_scroll = 0;
    }
    /// Go to line end (max horizontal scroll), like vim `$`.
    pub fn go_to_line_end(&mut self) {
        // Find the max line length in the current viewport
        let max_len = self.viewport_lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        // Content width = terminal width minus line numbers (~6 chars) and borders (~2 chars)
        let content_width = self.terminal_width.saturating_sub(8) as usize;
        let visible_chars = content_width.max(20); // at least 20 chars
        self.horizontal_scroll = max_len.saturating_sub(visible_chars);
    }

    pub fn page_down(&mut self) { self.scroll_down(40); }
    pub fn page_up(&mut self) { self.scroll_up(40); }

    // ── Search history ──

    fn history_path() -> std::path::PathBuf {
        std::path::PathBuf::from(".log_analyzer").join("search_history.json")
    }

    fn load_search_history() -> Vec<String> {
        let path = Self::history_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(&data) {
                return list.into_iter().take(100).collect();
            }
        }
        Vec::new()
    }

    fn save_search_history(&self) {
        let path = Self::history_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string(&self.search_history).unwrap_or_default();
        let _ = std::fs::write(&path, json);
    }

    /// Add a search term to history (dedup, most recent first, capped at 100).
    pub fn add_to_history(&mut self, term: &str) {
        let term = term.trim().to_string();
        if term.is_empty() {
            return;
        }
        self.search_history.retain(|t| t != &term);
        self.search_history.insert(0, term);
        self.search_history.truncate(100);
        self.save_search_history();
    }

    /// Step up/back in search history. Returns the term to fill in.
    pub fn history_navigate_up(&mut self) -> Option<&str> {
        if self.search_history.is_empty() {
            return None;
        }
        let next = (self.search_history_idx + 1).min(self.search_history.len() as isize - 1);
        self.search_history_idx = next;
        self.search_history.get(next as usize).map(|s| s.as_str())
    }

    /// Step down/forward in search history. Returns the term or empty.
    pub fn history_navigate_down(&mut self) -> Option<&str> {
        if self.search_history_idx <= 0 {
            self.search_history_idx = -1;
            return Some("");
        }
        self.search_history_idx -= 1;
        self.search_history.get(self.search_history_idx as usize).map(|s| s.as_str())
    }

    /// Reset history navigation position.
    pub fn history_reset(&mut self) {
        self.search_history_idx = -1;
    }
}

/// Build a tree connector string like git log --graph.
/// ancestors: list of ancestor node_ids that have more children after the current one.
/// When `ascii_only` is true, uses ASCII characters (|, `--, `--) instead of
/// Unicode box-drawing (│, ├─, └─) for terminals that don't support UTF-8.
fn build_connector(depth: usize, ancestors: &[usize], has_children: bool, ascii_only: bool) -> String {
    if depth == 0 {
        return String::new();
    }
    let mut s = String::with_capacity(depth * 2);
    for d in 0..depth - 1 {
        let has_continuing = ancestors.len() > d;
        if has_continuing {
            if ascii_only {
                s.push_str("| ");
            } else {
                s.push_str("│ ");
            }
        } else {
            s.push_str("  ");
        }
    }
    // Last level
    if has_children {
        if ascii_only {
            s.push_str("|-");
        } else {
            s.push_str("├─");
        }
    } else {
        if ascii_only {
            s.push_str("`-");
        } else {
            s.push_str("└─");
        }
    }
    s
}

fn set_tmux_title(title: &str) {
    // ANSI escape to set terminal title (works in tmux too)
    print!("\x1b]2;{}\x07", title);
    // Also set tmux pane title
    print!("\x1b]0;{}\x07", title);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_test_log(lines: usize) -> String {
        (0..lines)
            .map(|i| {
                let level = match i % 4 {
                    0 => "INFO",
                    1 => "WARN",
                    2 => "ERROR",
                    3 => "DEBUG",
                    _ => "unknown",
                };
                format!("2024-01-01 00:00:{:02} [{}] message {}", i % 60, level, i)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn setup_app(tmp: &TempDir) -> App {
        let log_file = tmp.path().join("test.log");
        let content = make_test_log(200);
        fs::write(&log_file, &content).unwrap();
        let ws_root = tmp.path().join("workspace");
        let ws = Workspace::open(&ws_root).unwrap();
        let _ = ws.migrate_if_needed();
        ws.import_file("test", &log_file).unwrap();
        App::new(&ws_root, Some("test")).unwrap()
    }

    // ── App creation ──

    #[test]
    fn test_app_new_with_repo() {
        let tmp = TempDir::new().unwrap();
        let app = setup_app(&tmp);

        assert_eq!(app.repo_name, "test");
        assert_eq!(app.total_lines, 200);
        assert!(!app.viewport_lines.is_empty());
        assert_eq!(app.active_view, ViewKind::LogView);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_app_new_empty_workspace() {
        let tmp = TempDir::new().unwrap();
        let ws_root = tmp.path().join("empty_ws");
        fs::create_dir_all(&ws_root).unwrap();
        let app = App::new(&ws_root, None).unwrap();

        assert!(app.repo_name.is_empty());
        assert_eq!(app.total_lines, 0);
        assert!(app.viewport_lines.is_empty());
    }

    // ── Search ──

    #[test]
    fn test_do_search_finds_matches() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        app.do_search("ERROR");
        assert!(!app.search_results.is_empty());
        assert_eq!(app.search_query, "ERROR");
        // Should jump to first match
        assert!(app.cursor_line < 200);
    }

    #[test]
    fn test_do_search_no_match() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        app.do_search("NO_SUCH_TEXT_ANYWHERE");
        assert!(app.search_results.is_empty());
        assert_eq!(app.status_message, "No matches found");
    }

    #[test]
    fn test_next_and_prev_match() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        app.do_search("message 5");
        assert!(!app.search_results.is_empty());
        let first = app.cursor_line;

        app.next_match();
        assert_ne!(app.cursor_line, first);
        let second = app.cursor_line;

        app.prev_match();
        assert_eq!(app.cursor_line, first);

        app.prev_match(); // wraps around to last match
        assert_ne!(app.cursor_line, first);
        app.next_match(); // wraps around to first again
        assert_eq!(app.cursor_line, first);
    }

    // ── Filter ──

    #[test]
    fn test_filter_keep_reduces_lines() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        assert_eq!(app.total_lines, 200);
        app.queue_operation(Operation::Filter {
            pattern: "ERROR".to_string(),
            keep: true,
        });
        app.apply_pending();

        // ERROR is every 4th line → 50 lines
        assert_eq!(app.total_lines, 50);
        assert!(app.viewport_lines.iter().all(|l| l.contains("ERROR")));
    }

    #[test]
    fn test_filter_remove() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        app.queue_operation(Operation::Filter {
            pattern: "ERROR".to_string(),
            keep: false,
        });
        app.apply_pending();

        // 200 - 50 = 150 lines
        assert_eq!(app.total_lines, 150);
        assert!(!app.viewport_lines.iter().any(|l| l.contains("ERROR")));
    }

    // ── Undo ──

    #[test]
    fn test_undo_restores_state() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        app.queue_operation(Operation::Filter {
            pattern: "ERROR".to_string(),
            keep: true,
        });
        app.apply_pending();
        assert_eq!(app.total_lines, 50);
        assert!(!app.line_count_is_original);

        app.queue_undo();
        app.apply_pending();
        assert_eq!(app.total_lines, 200);
        // Note: line_count_is_original may stay false with non-destructive undo
        // since history tree nodes are preserved
    }

    // ── Scroll offset clamping (regression test for the blank-viewport bug) ──

    #[test]
    fn test_scroll_offset_clamped_after_filter() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        // Simulate a search that moves scroll far down
        app.scroll_offset = 150;
        assert!(app.scroll_offset > 0);

        // Apply a filter that drastically reduces the dataset
        app.queue_operation(Operation::Filter {
            pattern: "ERROR".to_string(),
            keep: true,
        });
        app.apply_pending();

        // After load_viewport, scroll_offset should be clamped to < total_lines
        assert!(app.scroll_offset < app.total_lines);
        assert!(!app.viewport_lines.is_empty());
        assert!(app.cursor_line < app.total_lines);
    }

    #[test]
    fn test_scroll_offset_clamped_to_zero_when_empty() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        app.scroll_offset = 50;

        // Filter to zero lines (pattern never matches)
        app.queue_operation(Operation::Filter {
            pattern: "ZZZ_NONEXISTENT_ZZZ".to_string(),
            keep: true,
        });
        app.apply_pending();

        assert_eq!(app.total_lines, 0);
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.cursor_line, 0);
    }

    // ── Horizontal scroll ──

    #[test]
    fn test_horizontal_scroll_left_right() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        assert_eq!(app.horizontal_scroll, 0);
        app.scroll_right(20);
        assert_eq!(app.horizontal_scroll, 20);
        app.scroll_left(8);
        assert_eq!(app.horizontal_scroll, 12);
        app.scroll_left(50);
        assert_eq!(app.horizontal_scroll, 0); // saturating_sub
    }

    #[test]
    fn test_go_to_line_start() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        app.scroll_right(50);
        assert!(app.horizontal_scroll > 0);
        app.go_to_line_start();
        assert_eq!(app.horizontal_scroll, 0);
    }

    #[test]
    fn test_go_to_line_end() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        // Ensure we have lines with some length
        assert!(!app.viewport_lines.is_empty());
        app.go_to_line_end();
        // Either 0 (all lines fit) or > 0 (some lines wider)
        // We can at least assert it doesn't panic
    }

    // ── Input modes ──

    #[test]
    fn test_input_mode_transitions() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        assert_eq!(app.input_mode, InputMode::Normal);

        // Enter search mode
        app.input_mode = InputMode::Search;
        app.input_buffer = "test_query".to_string();
        assert_eq!(app.input_mode, InputMode::Search);
        assert_eq!(app.input_buffer, "test_query");

        // Enter command mode
        app.input_mode = InputMode::Command;
        app.input_buffer = ":f ERROR".to_string();
        assert_eq!(app.input_mode, InputMode::Command);
    }

    // ── Search history ──

    #[test]
    fn test_add_to_history_dedup() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        // Clear any pre-loaded history for predictable tests
        app.search_history.clear();

        app.add_to_history("error");
        assert_eq!(app.search_history[0], "error");
        assert_eq!(app.search_history.len(), 1);

        app.add_to_history("warn");
        assert_eq!(app.search_history[0], "warn");
        assert_eq!(app.search_history[1], "error");
        assert_eq!(app.search_history.len(), 2);

        // Adding again moves to front without increasing length
        app.add_to_history("error");
        assert_eq!(app.search_history[0], "error");
        assert_eq!(app.search_history[1], "warn");
        assert_eq!(app.search_history.len(), 2);
    }

    #[test]
    fn test_history_navigation() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        // Clear pre-loaded history for predictable navigation
        app.search_history.clear();
        app.add_to_history("first");
        app.add_to_history("second");
        app.search_history_idx = -1;

        // History is most-recent-first: ["second", "first"]
        // Navigate up (older) → "second" first
        let result = app.history_navigate_up();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "second");

        // Navigate up again → "first"
        let result = app.history_navigate_up();
        assert_eq!(result.unwrap(), "first");

        // Navigate down → back to "second"
        let result = app.history_navigate_down();
        assert_eq!(result.unwrap(), "second");

        // Navigate down again → empty (past the newest)
        let result = app.history_navigate_down();
        assert_eq!(result.unwrap(), "");
    }

    // ── View switching ──

    #[test]
    fn test_view_switching() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        assert_eq!(app.active_view, ViewKind::LogView);

        app.active_view = ViewKind::History;
        assert_eq!(app.active_view, ViewKind::History);

        app.active_view = ViewKind::RepoList;
        assert_eq!(app.active_view, ViewKind::RepoList);

        app.active_view = ViewKind::Analytics;
        assert_eq!(app.active_view, ViewKind::Analytics);
    }

    // ── Help toggle ──

    #[test]
    fn test_help_toggle() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        assert!(!app.show_help);
        app.show_help = true;
        assert!(app.show_help);
        app.show_help = false;
        assert!(!app.show_help);
    }

    // ── Go to line ──

    #[test]
    fn test_go_to_line() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        app.go_to_line(50);
        assert_eq!(app.cursor_line, 50);
        // scroll_offset should be near the cursor
        assert!(app.scroll_offset <= 50);
    }

    #[test]
    fn test_go_to_line_clamped() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        app.go_to_line(9999); // beyond end
        assert_eq!(app.cursor_line, 199); // clamped to last line
    }

    // ── Page up/down ──

    #[test]
    fn test_page_down_and_up() {
        let tmp = TempDir::new().unwrap();
        let mut app = setup_app(&tmp);

        let initial = app.scroll_offset;
        app.page_down();
        assert!(app.scroll_offset > initial);
        app.page_up();
        assert_eq!(app.scroll_offset, initial);
    }
}
