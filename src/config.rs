use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cache::CacheConfig;

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

    /// Cache configuration.
    #[serde(default)]
    pub cache: CacheConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            filters: HashMap::new(),
            workspace_dir: None,
            default_page_size: None,
            show_hidden_files: false,
            cache: CacheConfig::default(),
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
        // Merge cache config
        self.cache.merge(&other.cache);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.filters.is_empty());
        assert!(config.workspace_dir.is_none());
        assert!(config.default_page_size.is_none());
        assert!(!config.show_hidden_files);
    }

    #[test]
    fn test_filter_lookup() {
        let mut config = Config::default();
        config.filters.insert(
            "5xx".to_string(),
            r#"" (5\d\d) ""#.to_string(),
        );
        config
            .filters
            .insert("err".to_string(), "ERROR".to_string());

        assert_eq!(
            config.get_filter("5xx"),
            Some(r#"" (5\d\d) ""#)
        );
        assert_eq!(config.get_filter("err"), Some("ERROR"));
        assert_eq!(config.get_filter("nonexistent"), None);
    }

    #[test]
    fn test_merge_overrides_filters() {
        let mut base = Config::default();
        base.filters
            .insert("key".to_string(), "old_value".to_string());

        let mut other = Config::default();
        other
            .filters
            .insert("key".to_string(), "new_value".to_string());
        other.workspace_dir = Some("/custom/workspace".to_string());
        other.default_page_size = Some(50);
        other.show_hidden_files = true;

        base.merge(other);
        assert_eq!(base.get_filter("key"), Some("new_value"));
        assert_eq!(base.workspace_dir, Some("/custom/workspace".to_string()));
        assert_eq!(base.default_page_size, Some(50));
        assert!(base.show_hidden_files);
    }

    #[test]
    fn test_merge_does_not_override_with_none() {
        let mut base = Config::default();
        base.workspace_dir = Some("/original".to_string());
        base.default_page_size = Some(30);

        let other = Config::default();
        base.merge(other);
        assert_eq!(base.workspace_dir, Some("/original".to_string()));
        assert_eq!(base.default_page_size, Some(30));
    }

    #[test]
    fn test_filter_names() {
        let mut config = Config::default();
        config
            .filters
            .insert("a".to_string(), "1".to_string());
        config
            .filters
            .insert("b".to_string(), "2".to_string());

        let mut names = config.filter_names();
        names.sort();
        assert_eq!(names, vec![&"a".to_string(), &"b".to_string()]);
    }
}
