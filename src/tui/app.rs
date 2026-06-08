use std::cell::RefCell;
use std::path::Path;

use log_analyzer_core::error::Result;
use log_analyzer_core::operator::Operation;
use log_analyzer_core::repo::{LogRepo, Workspace};

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ViewKind {
    LogView,
    RepoList,
    Analytics,
    Help,
}

#[derive(Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Command,
    Search,
    Input,
}

enum PendingOp {
    None,
    OpenRepo(String),
    ApplyOperation(Operation),
    Undo,
}

pub struct App {
    pub workspace: Workspace,
    pub repo: RefCell<Option<LogRepo>>,
    pub repo_name: String,
    pub active_view: ViewKind,
    pub scroll_offset: usize,
    pub cursor_line: usize,
    pub viewport_lines: Vec<String>,
    pub total_lines: usize,
    pub line_count_is_original: bool,
    pub search_query: String,
    pub search_results: Vec<usize>,
    pub search_index: usize,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub input_prompt: String,
    pub status_message: String,
    pub error_message: Option<String>,
    pub show_help: bool,
    pub should_quit: bool,
    pending_op: PendingOp,
}

impl App {
    pub fn new(workspace_root: &Path, initial_repo: Option<&str>) -> Result<Self> {
        let workspace = Workspace::open(workspace_root)?;
        let _ = workspace.migrate_if_needed();

        let mut app = Self {
            workspace,
            repo: RefCell::new(None),
            repo_name: String::new(),
            active_view: ViewKind::LogView,
            scroll_offset: 0,
            cursor_line: 0,
            viewport_lines: Vec::new(),
            total_lines: 0,
            line_count_is_original: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_index: 0,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            input_prompt: String::new(),
            status_message: String::new(),
            error_message: None,
            show_help: false,
            should_quit: false,
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
                String::from("No repos found. Press 'i' to import a log file.");
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
            }
            Err(e) => {
                self.error_message =
                    Some(format!("Failed to open repo '{}': {}", name, e));
            }
        }
    }

    pub fn load_viewport(&mut self) {
        // Read viewport lines
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

    pub fn queue_operation(&mut self, op: Operation) {
        self.pending_op = PendingOp::ApplyOperation(op);
    }

    pub fn queue_undo(&mut self) {
        self.pending_op = PendingOp::Undo;
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
            }
        }
    }

    pub fn do_search(&mut self, query: &str) {
        self.search_query = query.to_string();
        self.search_results.clear();
        self.search_index = 0;

        // Gather results in a scoped block so the Ref is dropped before load_viewport
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

    pub fn next_match(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        self.search_index = (self.search_index + 1) % self.search_results.len();
        self.jump_to_match();
    }

    pub fn prev_match(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
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
        if self.total_lines == 0 {
            return;
        }
        let max_scroll = self.total_lines.saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + n).min(max_scroll);
        if self.cursor_line < self.scroll_offset {
            self.cursor_line = self.scroll_offset;
        }
        self.load_viewport();
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        if self.cursor_line >= self.scroll_offset + 50 {
            self.cursor_line = (self.scroll_offset + 50).saturating_sub(1);
        }
        self.load_viewport();
    }

    pub fn go_to_line(&mut self, line: usize) {
        let max_line = self.total_lines.saturating_sub(1);
        self.cursor_line = line.min(max_line);
        if self.cursor_line < self.scroll_offset
            || self.cursor_line >= self.scroll_offset + 50
        {
            self.scroll_offset = self.cursor_line.saturating_sub(15);
        }
        self.load_viewport();
    }

    pub fn page_down(&mut self) {
        let n = 40usize;
        self.scroll_down(n);
    }

    pub fn page_up(&mut self) {
        let n = 40usize;
        self.scroll_up(n);
    }
}
