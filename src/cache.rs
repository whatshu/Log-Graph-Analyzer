use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Configuration for the multi-level caching system.
///
/// Each tier can override the previous. The effective max_size is the
/// first non-zero value found in: session → local → user → system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum cache size in megabytes. 0 = use parent tier.
    pub max_size_mb: Option<u64>,
    /// Node IDs to pin (never evict). Only applies at this tier.
    #[serde(default)]
    pub pinned_nodes: Vec<usize>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size_mb: None,
            pinned_nodes: Vec::new(),
        }
    }
}

impl CacheConfig {
    /// Merge another config into this one (other overrides if set).
    pub fn merge(&mut self, other: &CacheConfig) {
        if other.max_size_mb.is_some() {
            self.max_size_mb = other.max_size_mb;
        }
        for node in &other.pinned_nodes {
            if !self.pinned_nodes.contains(node) {
                self.pinned_nodes.push(*node);
            }
        }
    }

    /// Get the effective max size in bytes. Returns 0 if unlimited (no config set).
    pub fn max_size_bytes(&self) -> u64 {
        self.max_size_mb.unwrap_or(0) * 1024 * 1024
    }
}

/// Index entry for a cached node state.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheIndexEntry {
    repo_hash: String,
    node_id: usize,
    size_bytes: u64,
    last_access_secs: u64,
    pinned: bool,
}

/// On-disk cache index.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheIndex {
    entries: Vec<CacheIndexEntry>,
    total_size_bytes: u64,
}

/// Manages cached computed states for history nodes.
///
/// Cache files are stored in `.log_analyzer/cache/` as zstd-compressed
/// files named `{repo_hash}_{node_id}.zst`. The index file `index.json`
/// tracks metadata for LRU eviction.
pub struct CacheManager {
    cache_dir: PathBuf,
    index: CacheIndex,
    config: CacheConfig,
    /// Session-level pinned nodes (from `:cache pin` command).
    session_pinned: HashSet<usize>,
    /// Session-level max size override (from `:cache max` command).
    session_max_size_bytes: Option<u64>,
}

impl CacheManager {
    /// Create a new cache manager. The cache directory is created if needed.
    pub fn new(cache_dir: PathBuf, config: CacheConfig) -> Result<Self> {
        fs::create_dir_all(&cache_dir)?;

        let index_path = cache_dir.join("index.json");
        let index = if index_path.exists() {
            let data = fs::read_to_string(&index_path)?;
            serde_json::from_str(&data).unwrap_or(CacheIndex {
                entries: Vec::new(),
                total_size_bytes: 0,
            })
        } else {
            CacheIndex {
                entries: Vec::new(),
                total_size_bytes: 0,
            }
        };

        Ok(Self {
            cache_dir,
            index,
            config,
            session_pinned: HashSet::new(),
            session_max_size_bytes: None,
        })
    }

    /// Set session-level max cache size (overrides config).
    pub fn set_session_max_mb(&mut self, mb: u64) {
        self.session_max_size_bytes = Some(mb * 1024 * 1024);
    }

    /// Get the effective max cache size in bytes.
    pub fn effective_max_size(&self) -> u64 {
        self.session_max_size_bytes.unwrap_or_else(|| self.config.max_size_bytes())
    }

    /// Pin a node so it's never evicted.
    pub fn pin(&mut self, node_id: usize) {
        self.session_pinned.insert(node_id);
        // Also update index entry if it exists
        if let Some(entry) = self.index.entries.iter_mut().find(|e| e.node_id == node_id) {
            entry.pinned = true;
        }
    }

    /// Unpin a node.
    pub fn unpin(&mut self, node_id: usize) {
        self.session_pinned.remove(&node_id);
        if let Some(entry) = self.index.entries.iter_mut().find(|e| e.node_id == node_id) {
            entry.pinned = false;
        }
    }

    /// Clear all cached entries for a specific repository.
    /// Call this after operations that change the history tree (merge, undo, etc.).
    pub fn clear_repo(&mut self, repo_hash: &str) {
        let paths: Vec<PathBuf> = self
            .index
            .entries
            .iter()
            .filter(|e| e.repo_hash == repo_hash)
            .map(|e| self.cache_path(&e.repo_hash, e.node_id))
            .collect();
        for path in &paths {
            let _ = fs::remove_file(path);
        }
        let removed_size: u64 = self
            .index
            .entries
            .iter()
            .filter(|e| e.repo_hash == repo_hash)
            .map(|e| e.size_bytes)
            .sum();
        self.index.total_size_bytes = self.index.total_size_bytes.saturating_sub(removed_size);
        self.index.entries.retain(|e| e.repo_hash != repo_hash);
        let _ = self.save_index();
    }

    /// Check if a cached state exists for a node.
    pub fn has(&self, repo_hash: &str, node_id: usize) -> bool {
        let path = self.cache_path(repo_hash, node_id);
        path.exists()
    }

    /// Get cached lines for a node. Returns None if not cached.
    pub fn get(&mut self, repo_hash: &str, node_id: usize) -> Option<Vec<String>> {
        let path = self.cache_path(repo_hash, node_id);
        if !path.exists() {
            return None;
        }

        // Read compressed data
        let compressed = fs::read(&path).ok()?;
        let mut decoder = zstd::Decoder::new(compressed.as_slice()).ok()?;
        let mut data = Vec::new();
        decoder.read_to_end(&mut data).ok()?;

        let content = String::from_utf8_lossy(&data).to_string();
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        // Update access time
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Some(entry) = self
            .index
            .entries
            .iter_mut()
            .find(|e| e.repo_hash == repo_hash && e.node_id == node_id)
        {
            entry.last_access_secs = now;
        }

        Some(lines)
    }

    /// Store computed lines for a node in the cache.
    /// Automatically evicts entries if needed to stay under max_size.
    pub fn put(&mut self, repo_hash: &str, node_id: usize, lines: &[String]) -> Result<()> {
        let content = lines.join("\n");
        let raw = content.as_bytes();

        // Compress with zstd
        let compressed = zstd::encode_all(raw, 3).map_err(|e| {
            crate::error::LogAnalyzerError::Compression(format!("Cache compression failed: {}", e))
        })?;

        let path = self.cache_path(repo_hash, node_id);
        fs::write(&path, &compressed)?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let is_pinned = self.config.pinned_nodes.contains(&node_id)
            || self.session_pinned.contains(&node_id);

        // Update or insert index entry
        let compressed_size = compressed.len() as u64;
        if let Some(entry) = self
            .index
            .entries
            .iter_mut()
            .find(|e| e.repo_hash == repo_hash && e.node_id == node_id)
        {
            self.index.total_size_bytes = self
                .index
                .total_size_bytes
                .saturating_sub(entry.size_bytes)
                + compressed_size;
            entry.size_bytes = compressed_size;
            entry.last_access_secs = now;
            entry.pinned = is_pinned;
        } else {
            self.index.total_size_bytes += compressed_size;
            self.index.entries.push(CacheIndexEntry {
                repo_hash: repo_hash.to_string(),
                node_id,
                size_bytes: compressed_size,
                last_access_secs: now,
                pinned: is_pinned,
            });
        }

        // Evict if over limit
        self.evict_if_needed()?;

        // Save index
        self.save_index()?;

        Ok(())
    }

    /// Evict entries using LRU strategy until under max_size.
    /// Pinned entries are never evicted.
    fn evict_if_needed(&mut self) -> Result<()> {
        let max_size = self.effective_max_size();
        if max_size == 0 {
            return Ok(()); // no limit
        }

        while self.index.total_size_bytes > max_size {
            // Find the least recently used non-pinned entry
            let idx = self
                .index
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.pinned)
                .min_by_key(|(_, e)| e.last_access_secs)
                .map(|(i, _)| i);

            match idx {
                Some(i) => {
                    let entry = &self.index.entries[i];
                    let path = self.cache_path(&entry.repo_hash, entry.node_id);
                    let _ = fs::remove_file(&path);
                    self.index.total_size_bytes -= entry.size_bytes;
                    self.index.entries.remove(i);
                }
                None => {
                    // All entries are pinned — can't evict. Stop.
                    break;
                }
            }
        }

        Ok(())
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.index.entries.len(),
            total_size_bytes: self.index.total_size_bytes,
            max_size_bytes: self.effective_max_size(),
            pinned_count: self
                .index
                .entries
                .iter()
                .filter(|e| e.pinned)
                .count(),
        }
    }

    fn cache_path(&self, repo_hash: &str, node_id: usize) -> PathBuf {
        self.cache_dir.join(format!("{}_{}.zst", repo_hash, node_id))
    }

    fn save_index(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.index)?;
        fs::write(self.cache_dir.join("index.json"), json)?;
        Ok(())
    }
}

/// Cache statistics for display.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entry_count: usize,
    pub total_size_bytes: u64,
    pub max_size_bytes: u64,
    pub pinned_count: usize,
}

impl CacheStats {
    pub fn total_size_mb(&self) -> f64 {
        self.total_size_bytes as f64 / (1024.0 * 1024.0)
    }

    pub fn max_size_mb(&self) -> f64 {
        self.max_size_bytes as f64 / (1024.0 * 1024.0)
    }
}

/// Compute a hash for a repository path (simple hash for cache key).
pub fn hash_repo_path(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    // Use a simple fnv-like hash
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in path_str.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}
