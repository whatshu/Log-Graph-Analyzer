use std::path::{Path, PathBuf};
use std::fs;

/// A ranger-inspired file browser widget for the TUI.
pub struct FileBrowser {
    pub current_dir: PathBuf,
    pub entries: Vec<FsEntry>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub show_hidden: bool,
    pub filter: String,
    pub preview: Vec<String>,
    pub message: Option<String>,
}

#[derive(Clone)]
pub struct FsEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub _selected: bool,
}

impl FileBrowser {
    pub fn new(start_dir: &Path) -> Self {
        let current_dir = if start_dir.is_dir() {
            start_dir.to_path_buf()
        } else {
            start_dir.parent().unwrap_or(Path::new(".")).to_path_buf()
        };
        let mut fb = Self {
            current_dir,
            entries: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            show_hidden: false,
            filter: String::new(),
            preview: Vec::new(),
            message: None,
        };
        fb.refresh();
        fb
    }

    pub fn refresh(&mut self) {
        self.entries.clear();
        // Parent directory entry
        if let Some(_parent) = self.current_dir.parent() {
            self.entries.push(FsEntry {
                name: String::from(".."),
                is_dir: true,
                size: 0,
                _selected: false,
            });
        }

        let dir_iter = match fs::read_dir(&self.current_dir) {
            Ok(it) => it,
            Err(e) => {
                self.message = Some(format!("Cannot read dir: {}", e));
                return;
            }
        };

        let mut raw: Vec<FsEntry> = Vec::new();
        for entry in dir_iter.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Filter hidden files
            if !self.show_hidden && name.starts_with('.') {
                continue;
            }
            // Apply filter
            if !self.filter.is_empty() && !name.to_lowercase().contains(&self.filter.to_lowercase())
            {
                continue;
            }
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let size = if is_dir {
                0
            } else {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            };
            raw.push(FsEntry {
                name,
                is_dir,
                size,
                _selected: false,
            });
        }

        // Sort: directories first, then alphabetical
        raw.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        self.entries.extend(raw);

        if self.selected_index >= self.entries.len() {
            self.selected_index = self.entries.len().saturating_sub(1);
        }
        self.load_preview();
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.adjust_scroll();
            self.load_preview();
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.entries.len() {
            self.selected_index += 1;
            self.adjust_scroll();
            self.load_preview();
        }
    }

    pub fn enter_dir(&mut self) -> bool {
        if self.selected_index >= self.entries.len() {
            return false;
        }
        let entry = &self.entries[self.selected_index];
        if entry.is_dir {
            let new_dir = if entry.name == ".." {
                self.current_dir.parent().map(|p| p.to_path_buf())
            } else {
                Some(self.current_dir.join(&entry.name))
            };
            if let Some(dir) = new_dir {
                self.current_dir = dir;
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.refresh();
            }
            false // still browsing
        } else {
            true // file selected
        }
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        if self.selected_index >= self.entries.len() {
            return None;
        }
        let entry = &self.entries[self.selected_index];
        if entry.name == ".." {
            None
        } else {
            Some(self.current_dir.join(&entry.name))
        }
    }

    pub fn selected_entry_name(&self) -> Option<&str> {
        if self.selected_index >= self.entries.len() {
            return None;
        }
        Some(&self.entries[self.selected_index].name)
    }

    fn adjust_scroll(&mut self) {
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }
        // We'll let the UI decide visible count
    }

    fn load_preview(&mut self) {
        self.preview.clear();
        if let Some(path) = self.selected_path() {
            if !path.is_file() {
                return;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                for (i, line) in content.lines().take(20).enumerate() {
                    self.preview.push(format!("{:>4} | {}", i + 1, line));
                }
                if content.lines().count() > 20 {
                    self.preview.push(format!(
                        "  ... ({} more lines)",
                        content.lines().count() - 20
                    ));
                }
            }
        }
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.refresh();
    }

    #[allow(dead_code)]
    pub fn set_filter(&mut self, f: String) {
        self.filter = f;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.refresh();
    }
}

pub fn render_file_browser(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    fb: &FileBrowser,
    ascii_only: bool,
) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let chunks = if area.width > 80 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100), Constraint::Percentage(0)])
            .split(area)
    };

    // Left pane: directory listing
    let path_str = fb.current_dir.display().to_string();
    let title = format!(
        " {} {}",
        if fb.show_hidden { "[all]" } else { "[ ]" },
        path_str
    );

    let visible_count = chunks[0].height.saturating_sub(2) as usize;
    let start = fb.scroll_offset.min(fb.entries.len().saturating_sub(1));
    let end = (start + visible_count).min(fb.entries.len());

    let dir_lines: Vec<Line> = fb.entries[start..end]
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let idx = start + i;
            let is_selected = idx == fb.selected_index;
            let icon = if entry.is_dir {
                if ascii_only { "[D] " } else { "📁 " }
            } else {
                if ascii_only { "[F] " } else { "📄 " }
            };
            let size_str = if entry.is_dir {
                String::from("<DIR>")
            } else {
                format_size(entry.size)
            };

            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if entry.is_dir {
                Style::default().fg(Color::Blue)
            } else {
                Style::default().fg(Color::White)
            };

            Line::from(Span::styled(
                format!(" {}{:<40} {:>8}", icon, entry.name, size_str),
                style,
            ))
        })
        .collect();

    let dir_p = Paragraph::new(dir_lines)
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(dir_p, chunks[0]);

    // Right pane: preview
    if chunks[1].width > 0 {
        let preview_title = if let Some(name) = fb.selected_entry_name() {
            format!(" Preview: {} ", name)
        } else {
            String::from(" Preview ")
        };

        let preview_lines: Vec<Line> = fb
            .preview
            .iter()
            .map(|l| Line::from(Span::styled(l.as_str(), Style::default().fg(Color::Gray))))
            .collect();

        let preview_p = Paragraph::new(preview_lines)
            .block(Block::default().borders(Borders::ALL).title(preview_title));
        f.render_widget(preview_p, chunks[1]);
    }

    // Message at bottom
    if let Some(ref msg) = fb.message {
        let msg_p = Paragraph::new(Span::styled(
            msg.as_str(),
            Style::default().fg(Color::Red),
        ));
        f.render_widget(msg_p, area);
    }
}

pub fn render_file_browser_hints(f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let hints = vec![
        ("↑↓", "navigate"),
        ("Enter", "open dir/file"),
        ("/", "filter"),
        (".", "toggle hidden"),
        ("Esc", "cancel"),
    ];

    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(
                    *key,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(":{}  ", desc)),
            ]
        })
        .collect();

    let p = Paragraph::new(Line::from(spans));
    f.render_widget(p, area);
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}
