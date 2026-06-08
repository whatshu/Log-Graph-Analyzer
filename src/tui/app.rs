use std::cell::RefCell;
use std::path::Path;

use log_analyzer_core::config::Config;
use log_analyzer_core::error::Result;
use log_analyzer_core::operator::Operation;
use log_analyzer_core::repo::{LogRepo, Workspace};

use super::file_browser::FileBrowser;

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ViewKind {
    LogView,
    RepoList,
    Analytics,
    History,
    FileBrowser,
    Help,
}

#[derive(Clone, PartialEq, Eq)]
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
    Undo,
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

    // File browser
    pub file_browser: FileBrowser,

    // Tmux
    pub in_tmux: bool,

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
}

impl App {
    pub fn new(workspace_root: &Path, initial_repo: Option<&str>) -> Result<Self> {
        let config = Config::load();
        let workspace = Workspace::open(workspace_root)?;
        let _ = workspace.migrate_if_needed();

        let in_tmux = std::env::var("TMUX").is_ok();

        let mut app = Self {
            config,
            workspace,
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
            file_browser: FileBrowser::new(Path::new(".")),
            in_tmux,
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
                self.line_count_is_original = repo.history().is_empty();
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
        let repo_ref = self.repo.borrow();
        if repo_ref.is_none() {
            self.viewport_lines.clear();
            return;
        }

        let has_ops = repo_ref.as_ref().map_or(false, |r| !r.history().is_empty());
        drop(repo_ref);

        if has_ops {
            let mut repo_mut = self.repo.borrow_mut();
            if let Some(ref mut r) = *repo_mut {
                self.viewport_lines = r
                    .read_current_lines(self.scroll_offset, 200)
                    .unwrap_or_default();
                self.total_lines = r.current_line_count().unwrap_or(0);
            }
        } else {
            let repo_ref = self.repo.borrow();
            if let Some(ref r) = *repo_ref {
                self.viewport_lines = r
                    .read_original_lines(self.scroll_offset, 200)
                    .unwrap_or_default();
                self.total_lines = r.original_line_count();
            }
        }
        self.line_count_is_original = !has_ops;
    }

    pub fn refresh_line_count(&mut self) {
        let repo_ref = self.repo.borrow();
        if let Some(ref r) = *repo_ref {
            if r.history().is_empty() {
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

    /// Build the history view nodes.
    pub fn build_history(&mut self) {
        self.history_nodes.clear();
        let repo_ref = self.repo.borrow();
        if let Some(ref r) = *repo_ref {
            let original_lines = r.original_line_count();
            self.history_nodes.push(HistoryNode {
                id: 0,
                description: format!("Import — {} lines", original_lines),
                line_count: original_lines,
                applied_at: String::from("—"),
            });

            for record in r.history() {
                let lines = self.compute_line_count_at_node(r, record.id);
                self.history_nodes.push(HistoryNode {
                    id: record.id + 1,
                    description: record.operation.describe(),
                    line_count: lines,
                    applied_at: record
                        .applied_at
                        .format("%Y-%m-%d %H:%M")
                        .to_string(),
                });
            }
        }
        self.history_cursor = self.history_nodes.len().saturating_sub(1);
    }

    fn compute_line_count_at_node(&self, repo: &LogRepo, op_id: usize) -> usize {
        // For the last operation, just get current line count
        let total_ops = repo.history().len();
        if op_id + 1 == total_ops {
            // Can't call current_line_count on &LogRepo — use compute_state_at
            if let Ok(state) = repo.compute_state_at(op_id) {
                return state.len();
            }
        }
        if let Ok(state) = repo.compute_state_at(op_id) {
            state.len()
        } else {
            0
        }
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
        self.pending_op = PendingOp::ApplyOperation(op);
    }

    pub fn queue_undo(&mut self) {
        self.pending_op = PendingOp::Undo;
    }

    pub fn queue_checkout(&mut self, node_idx: usize) {
        self.pending_op = PendingOp::CheckoutTo(node_idx);
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
                            if r.history().is_empty() {
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
                let mut repo_mut = self.repo.borrow_mut();
                if let Some(ref mut r) = *repo_mut {
                    let target_op_idx = if node_idx == 0 {
                        // Undo everything
                        while !r.history().is_empty() {
                            if let Err(e) = r.undo() {
                                self.error_message =
                                    Some(format!("Checkout failed: {}", e));
                                return;
                            }
                        }
                        self.status_message = String::from("Checked out to root");
                        self.line_count_is_original = true;
                        return;
                    } else {
                        // op_id in history starts at 0, node_idx is id+1, so target = node_idx-1
                        node_idx - 1
                    };
                    match r.checkout_to(target_op_idx) {
                        Ok(()) => {
                            self.status_message = format!(
                                "Checked out to operation {}",
                                target_op_idx
                            );
                            self.line_count_is_original = r.history().is_empty();
                        }
                        Err(e) => {
                            self.error_message =
                                Some(format!("Checkout failed: {}", e));
                        }
                    }
                }
                drop(repo_mut);
                self.refresh_line_count();
                self.load_viewport();
                self.re_search();
                self.build_history();
            }
            PendingOp::ExportFrom(node_idx, path) => {
                let repo_ref = self.repo.borrow();
                if let Some(ref r) = *repo_ref {
                    let result = if node_idx == 0 {
                        // Export original
                        r.read_all_original_lines()
                            .map(|lines| lines.join("\n"))
                    } else {
                        // Export at operation (node_idx - 1 since node 0 is import)
                        let op_idx = node_idx - 1;
                        r.compute_state_at(op_idx)
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
        }
    }

    pub fn do_search(&mut self, query: &str) {
        self.search_query = query.to_string();
        self.search_results.clear();
        self.search_index = 0;

        let results: Vec<usize> = {
            let repo_ref = self.repo.borrow();
            if repo_ref.is_none() {
                return;
            }

            let has_ops = repo_ref.as_ref().map_or(false, |r: &LogRepo| !r.history().is_empty());
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
            let repo_ref = self.repo.borrow();
            if repo_ref.is_none() {
                return;
            }

            let has_ops =
                repo_ref
                    .as_ref()
                    .map_or(false, |r: &LogRepo| !r.history().is_empty());
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
        let visible_chars = 60usize; // reasonable default for content area
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

fn set_tmux_title(title: &str) {
    // ANSI escape to set terminal title (works in tmux too)
    print!("\x1b]2;{}\x07", title);
    // Also set tmux pane title
    print!("\x1b]0;{}\x07", title);
}
