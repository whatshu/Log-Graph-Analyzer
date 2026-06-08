use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// User configuration for log-analyzer.
///
/// Loaded from three tiers (each overrides the previous):
/// 1. System:  `/etc/log-analyzer/config.toml`
/// 2. User:    `~/.log_analyzer/config.toml`
/// 3. Local:   `./.log_analyzer/config.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Saved filter patterns (name → regex).
    #[serde(default)]
    pub filters: HashMap<String, String>,

    /// Default workspace directory.
    #[serde(default)]
    pub workspace_dir: Option<String>,

    /// Default lines per page in TUI viewer.
    #[serde(default)]
    pub default_page_size: Option<usize>,

    /// Show hidden files in file browser by default.
    #[serde(default)]
    pub show_hidden_files: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            filters: HashMap::new(),
            workspace_dir: None,
            default_page_size: None,
            show_hidden_files: false,
        }
    }
}

impl Config {
    /// Load configuration from all three tiers and merge them.
    pub fn load() -> Self {
        let mut config = Config::default();

        // Tier 1: system
        if let Some(sys) = Self::load_file(&PathBuf::from("/etc/log-analyzer/config.toml")) {
            config.merge(sys);
        }

        // Tier 2: user
        if let Some(home) = dirs_next_home() {
            let user_path = home.join(".log_analyzer").join("config.toml");
            if let Some(user) = Self::load_file(&user_path) {
                config.merge(user);
            }
        }

        // Tier 3: local (current directory)
        if let Some(local) = Self::load_file(&PathBuf::from(".log_analyzer/config.toml")) {
            config.merge(local);
        }

        config
    }

    /// Look up a saved filter by name. Returns the regex pattern if found.
    pub fn get_filter(&self, name: &str) -> Option<&str> {
        self.filters.get(name).map(|s| s.as_str())
    }

    /// Get all saved filter names.
    pub fn filter_names(&self) -> Vec<&String> {
        self.filters.keys().collect()
    }

    fn load_file(path: &PathBuf) -> Option<Config> {
        let content = std::fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    fn merge(&mut self, other: Config) {
        // Merge filters: other's keys override ours
        for (k, v) in other.filters {
            self.filters.insert(k, v);
        }
        if other.workspace_dir.is_some() {
            self.workspace_dir = other.workspace_dir;
        }
        if other.default_page_size.is_some() {
            self.default_page_size = other.default_page_size;
        }
        // show_hidden_files: true overrides false
        if other.show_hidden_files {
            self.show_hidden_files = true;
        }
    }
}

fn dirs_next_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| {
            std::env::var("USERPROFILE") // Windows
        })
        .ok()
        .map(PathBuf::from)
}
