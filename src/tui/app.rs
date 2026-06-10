use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::path::PathBuf;

use lograph::cache::CacheManager;
use lograph::config::Config;
use lograph::engine::{CollectResult, Collector};
use lograph::error::Result;
use lograph::operator::Operation;
use lograph::repo::Workspace;
use lograph::tag::TagStore;

use super::file_browser::FileBrowser;

// Re-export core types from state module for backward-compatible imports.
pub use super::state::{
    detect_utf8_locale, set_tmux_title, App, HistoryNode, InputMode, PendingOp, ViewKind,
};

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
                    lograph::cache::CacheConfig::default(),
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
            repo_cursor: 0,
            history_cursor: 0,
            history_nodes: Vec::new(),
            viewed_node_id: None,
            detached_head: false,
            file_browser: FileBrowser::new(Path::new(".")),
            in_tmux,
            terminal_width: 80, // default, updated on first render
            ascii_only: !detect_utf8_locale(),
            pending_history_export: None,
            pending_repo_clone_src: None,
            collect_results: HashMap::new(),
            pending_collect_summary: None,
            collect_detail: None,
            show_collect_detail: false,
            history_marks: HashSet::new(),
            yanked_node_id: None,
            diff_base_node_id: None,
            tag_store: TagStore::load(workspace_root),
            show_tag_manager: false,
            tag_manager_cursor: 0,
            tag_manager_scroll: 0,
            tag_manager_h_scroll: 0,
            pending_tag_rename: None,
            show_merge_mode_popup: false,
            merge_mode_cursor: 0,
            merge_sources: Vec::new(),
            search_tag_start: None,
            search_tag_end: None,
            picking_search_tag: false,
            search_tag_pick_cursor: 0,
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
                    set_tmux_title(&format!("lograph: {}", name));
                }
            }
            Err(e) => {
                self.error_message =
                    Some(format!("Failed to open repo '{}': {}", name, e));
            }
        }
    }

    pub fn load_viewport(&mut self) {
        super::viewport::load_viewport(self);
    }

    /// Clamp scroll_offset and cursor_line to the current total_lines range.
    fn clamp_scroll_state(&mut self) {
        super::viewport::clamp_scroll_state(self);
    }

    pub fn refresh_line_count(&mut self) {
        super::viewport::refresh_line_count(self);
    }

    /// Build the history view nodes from the tree topological order.
    pub fn build_history(&mut self) {
        super::history_builder::build_history(self);
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

    #[allow(dead_code)]
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
                .map(|r| lograph::cache::hash_repo_path(r.path()))
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
                return Err(lograph::error::LogAnalyzerError::Repo(
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
        self.clear_search();
    }

    pub fn queue_export_from(&mut self, node_idx: usize, path: String) {
        self.pending_op = PendingOp::ExportFrom(node_idx, path);
    }

    // ── Tag remapping ──

    /// Remap tag ranges after a non-destructive operation.
    /// `old_lines` is the pre-operation line set.
    pub(crate) fn remap_tags_after_operation(&mut self, op: &Operation, old_lines: &[String]) {
        super::tag_ops::remap_tags_after_operation(self, op, old_lines);
    }

    pub fn apply_pending(&mut self) {
        let pending = std::mem::replace(&mut self.pending_op, PendingOp::None);
        match pending {
            PendingOp::None => {}
            PendingOp::OpenRepo(name) => self.do_open_repo(&name),
            PendingOp::ApplyOperation(op) => {
                super::ops::apply_operation::execute(self, op);
            }
            PendingOp::Undo => {
                super::ops::undo::execute(self);
            }
            PendingOp::CheckoutTo(node_idx) => {
                super::ops::checkout::execute(self, node_idx);
            }
            PendingOp::ExportFrom(node_idx, path) => {
                super::ops::export::execute(self, node_idx, path);
            }
            PendingOp::ApplyOperationFrom(from_node_id, op) => {
                super::ops::apply_from::execute(self, from_node_id, op);
            }
            PendingOp::MergeNodes { sources, branch, mode } => {
                super::ops::merge::execute(self, sources, branch, mode);
            }
            PendingOp::SubtractNodes { base, subtrahend, branch } => {
                super::ops::subtract::execute(self, base, subtrahend, branch);
            }
            PendingOp::ReplayNode { source, target_parent, branch } => {
                super::ops::replay::execute(self, source, target_parent, branch);
            }
            PendingOp::SoftDelete { node_id } => {
                super::ops::soft_delete::execute(self, node_id);
            }
        }
    }

    pub fn do_search(&mut self, query: &str) {
        super::search::do_search(self, query);
    }

    /// Clear the current search state (query, results, and index).
    /// Called after operations that change the data so highlights don't
    /// persist onto the new dataset.
    pub fn clear_search(&mut self) {
        super::search::clear_search(self);
    }

    pub fn next_match(&mut self) {
        super::search::next_match(self);
    }

    pub fn prev_match(&mut self) {
        super::search::prev_match(self);
    }

    fn jump_to_match(&mut self) {
        super::search::jump_to_match(self);
    }

    pub fn scroll_down(&mut self, n: usize) {
        super::viewport::scroll_down(self, n);
    }

    pub fn scroll_up(&mut self, n: usize) {
        super::viewport::scroll_up(self, n);
    }

    pub fn go_to_line(&mut self, line: usize) {
        super::viewport::go_to_line(self, line);
    }

    pub fn scroll_right(&mut self, n: usize) {
        super::viewport::scroll_right(self, n);
    }
    pub fn scroll_left(&mut self, n: usize) {
        super::viewport::scroll_left(self, n);
    }
    /// Go to line start (horizontal 0), like vim `0`.
    pub fn go_to_line_start(&mut self) {
        super::viewport::go_to_line_start(self);
    }
    /// Go to line end (max horizontal scroll), like vim `$`.
    pub fn go_to_line_end(&mut self) {
        super::viewport::go_to_line_end(self);
    }

    pub fn page_down(&mut self) { self.scroll_down(40); }
    pub fn page_up(&mut self) { self.scroll_up(40); }

    // ── Collect operations ──

    /// Run a collector against the current repo state and create a history node.
    pub fn run_collect(&mut self, collector: Collector) {
        super::collect::run_collect(self, collector);
    }

    /// Format a one-line summary of a collect result.
    pub fn collect_result_summary(result: &CollectResult) -> String {
        super::collect::collect_result_summary(result)
    }

    /// Format a multi-line detail display of a collect result.
    pub fn collect_result_detail(result: &CollectResult) -> String {
        super::collect::collect_result_detail(result)
    }

    // ── Search history ──

    fn history_path() -> std::path::PathBuf {
        super::history_builder::history_path()
    }

    fn load_search_history() -> Vec<String> {
        super::history_builder::load_search_history()
    }

    fn save_search_history(&self) {
        super::history_builder::save_search_history(self);
    }

    /// Add a search term to history (dedup, most recent first, capped at 100).
    pub fn add_to_history(&mut self, term: &str) {
        super::history_builder::add_to_history(self, term);
    }

    /// Step up/back in search history. Returns the term to fill in.
    pub fn history_navigate_up(&mut self) -> Option<&str> {
        super::history_builder::history_navigate_up(self)
    }

    /// Step down/forward in search history. Returns the term or empty.
    pub fn history_navigate_down(&mut self) -> Option<&str> {
        super::history_builder::history_navigate_down(self)
    }

    /// Reset history navigation position.
    pub fn history_reset(&mut self) {
        super::history_builder::history_reset(self);
    }
}
