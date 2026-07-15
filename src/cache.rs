use std::collections::HashMap;
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};
use crate::error::Result;
use crate::download::VideoInfo;

const MAX_CACHE_SIZE_BYTES: u64 = 10 * 1024 * 1024 * 1024; // 10GB
const MANIFEST_FILE: &str = "index.json";

// ── Cache Entry ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    url: String,
    key: String,
    cached_at: u64,
    accessed_at: u64,
    size_bytes: u64,
    has_video: bool,
    has_subtitles: bool,
    title: Option<String>,
}

// ── VideoCache ───────────────────────────────────────────────────────────

/// File-based cache for video downloads and subtitles.
pub struct VideoCache {
    root: PathBuf,
    manifest: HashMap<String, CacheEntry>,
    max_size_bytes: u64,
}

impl VideoCache {
    /// Create or open the cache at the default location (~/.cache/watch2/).
    pub fn new() -> Result<Self> {
        let root = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("watch2");
        Self::with_dir(root)
    }

    /// Create or open the cache at a custom location.
    pub fn with_dir(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        let manifest_path = root.join(MANIFEST_FILE);
        let manifest = if manifest_path.exists() {
            let data = std::fs::read_to_string(&manifest_path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Self {
            root,
            manifest,
            max_size_bytes: MAX_CACHE_SIZE_BYTES,
        })
    }

    /// Generate cache key from URL (SHA256 of normalized URL).
    pub fn cache_key(url: &str) -> String {
        let normalized = normalize_url(url);
        let mut hasher = Sha256::new();
        hasher.update(normalized.as_bytes());
        let result = hasher.finalize();
        hex::encode(result)
    }

    /// Get the cache directory for a given key.
    pub fn cache_dir(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }

    // ── Subtitle Cache ──────────────────────────────────────────────

    /// Check if subtitles are cached for this URL.
    pub fn has_subtitles(&self, url: &str) -> bool {
        let key = Self::cache_key(url);
        self.manifest
            .get(&key)
            .map(|e| e.has_subtitles)
            .unwrap_or(false)
    }

    /// Get cached subtitle path if available.
    pub fn get_subtitles(&self, url: &str, lang: &str) -> Option<PathBuf> {
        let key = Self::cache_key(url);
        let entry = self.manifest.get(&key)?;
        if !entry.has_subtitles {
            return None;
        }
        let dir = self.cache_dir(&key);

        // Try original subs first, then auto-generated
        for suffix in &[format!("{lang}-orig"), lang.to_string()] {
            let path = dir.join(format!("video.{}.json3", suffix));
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// Store subtitle file in cache.
    pub fn store_subtitles(&mut self, url: &str, _lang: &str, path: &Path) -> Result<PathBuf> {
        let key = Self::cache_key(url);
        let dir = self.cache_dir(&key);
        std::fs::create_dir_all(&dir)?;

        let dest = dir.join(path.file_name().unwrap_or_default());
        std::fs::copy(path, &dest)?;

        // Update manifest
        let entry = self.manifest.entry(key.clone()).or_insert_with(|| CacheEntry {
            url: url.to_string(),
            key: key.clone(),
            cached_at: now_unix(),
            accessed_at: now_unix(),
            size_bytes: 0,
            has_video: false,
            has_subtitles: false,
            title: None,
        });
        entry.accessed_at = now_unix();
        entry.has_subtitles = true;
        entry.size_bytes = calc_dir_size(&dir);
        self.save_manifest()?;

        Ok(dest)
    }

    // ── Video Cache ─────────────────────────────────────────────────

    /// Check if video is cached for this URL.
    pub fn has_video(&self, url: &str) -> bool {
        let key = Self::cache_key(url);
        self.manifest
            .get(&key)
            .map(|e| e.has_video)
            .unwrap_or(false)
    }

    /// Get cached video path if available.
    pub fn get_video(&self, url: &str) -> Option<PathBuf> {
        let key = Self::cache_key(url);
        let entry = self.manifest.get(&key)?;
        if !entry.has_video {
            return None;
        }
        let path = self.cache_dir(&key).join("video.mp4");
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Store video file in cache.
    pub fn store_video(&mut self, url: &str, path: &Path) -> Result<PathBuf> {
        let key = Self::cache_key(url);
        let dir = self.cache_dir(&key);
        std::fs::create_dir_all(&dir)?;

        let dest = dir.join("video.mp4");
        std::fs::copy(path, &dest)?;

        // Update manifest
        let entry = self.manifest.entry(key.clone()).or_insert_with(|| CacheEntry {
            url: url.to_string(),
            key: key.clone(),
            cached_at: now_unix(),
            accessed_at: now_unix(),
            size_bytes: 0,
            has_video: false,
            has_subtitles: false,
            title: None,
        });
        entry.accessed_at = now_unix();
        entry.has_video = true;
        entry.size_bytes = calc_dir_size(&dir);
        self.save_manifest()?;

        // Evict if over limit
        let _ = self.evict();

        Ok(dest)
    }

    // ── Info Cache ──────────────────────────────────────────────────

    /// Get cached metadata if available.
    pub fn get_info(&self, url: &str) -> Option<VideoInfo> {
        let key = Self::cache_key(url);
        let path = self.cache_dir(&key).join("info.json");
        if !path.exists() {
            return None;
        }
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Store metadata in cache.
    pub fn store_info(&mut self, url: &str, info: &VideoInfo) -> Result<()> {
        let key = Self::cache_key(url);
        let dir = self.cache_dir(&key);
        std::fs::create_dir_all(&dir)?;

        let path = dir.join("info.json");
        let data = serde_json::to_string_pretty(info)?;
        std::fs::write(&path, data)?;

        // Update manifest
        let entry = self.manifest.entry(key.clone()).or_insert_with(|| CacheEntry {
            url: url.to_string(),
            key: key.clone(),
            cached_at: now_unix(),
            accessed_at: now_unix(),
            size_bytes: 0,
            has_video: false,
            has_subtitles: false,
            title: Some(info.title.clone()),
        });
        entry.accessed_at = now_unix();
        entry.title = Some(info.title.clone());
        entry.size_bytes = calc_dir_size(&dir);
        self.save_manifest()?;

        Ok(())
    }

    // ── Invalidation & Eviction ─────────────────────────────────────

    /// Remove a specific URL from cache.
    pub fn invalidate(&mut self, url: &str) -> Result<()> {
        let key = Self::cache_key(url);
        if let Some(entry) = self.manifest.remove(&key) {
            let dir = self.cache_dir(&entry.key);
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
            }
            self.save_manifest()?;
        }
        Ok(())
    }

    /// Evict oldest entries until total size is under max_size_bytes.
    pub fn evict(&mut self) -> Result<Vec<String>> {
        let mut evicted = Vec::new();

        while self.total_size() > self.max_size_bytes && !self.manifest.is_empty() {
            // Find the least recently accessed entry
            if let Some(oldest_key) = self.manifest
                .iter()
                .min_by_key(|(_, e)| e.accessed_at)
                .map(|(k, _)| k.clone())
            {
                if let Some(entry) = self.manifest.remove(&oldest_key) {
                    let dir = self.cache_dir(&entry.key);
                    if dir.exists() {
                        std::fs::remove_dir_all(&dir)?;
                    }
                    evicted.push(entry.url);
                }
            } else {
                break;
            }
        }

        if !evicted.is_empty() {
            self.save_manifest()?;
        }

        Ok(evicted)
    }

    /// Get total cache size in bytes.
    pub fn total_size(&self) -> u64 {
        self.manifest.values().map(|e| e.size_bytes).sum()
    }

    /// Get number of cached entries.
    pub fn entry_count(&self) -> usize {
        self.manifest.len()
    }

    /// Print cache stats to stderr.
    pub fn print_stats(&self) {
        let size_mb = self.total_size() as f64 / (1024.0 * 1024.0);
        let entries = self.entry_count();
        eprintln!("[watch2] cache: {} entries, {:.1} MB used (max {:.0} GB)",
            entries, size_mb, self.max_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0));
    }

    // ── Internal ────────────────────────────────────────────────────

    fn save_manifest(&self) -> Result<()> {
        let path = self.root.join(MANIFEST_FILE);
        let data = serde_json::to_string_pretty(&self.manifest)?;
        std::fs::write(&path, data)?;
        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Normalize a URL for cache key generation.
fn normalize_url(url: &str) -> String {
    let mut normalized = url.to_string();

    // Strip YouTube tracking params
    if let Some(pos) = normalized.find('?') {
        let base = &normalized[..pos];
        let query = &normalized[pos + 1..];
        let params: Vec<&str> = query.split('&')
            .filter(|p| !p.starts_with("si=") && !p.starts_with("list="))
            .collect();
        if params.is_empty() {
            normalized = base.to_string();
        } else {
            normalized = format!("{}?{}", base, params.join("&"));
        }
    }

    // Normalize YouTube URLs
    if normalized.contains("youtu.be/") {
        normalized = normalized
            .replace("youtu.be/", "www.youtube.com/watch?v=");
    }

    normalized
}

/// Get current Unix timestamp.
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Calculate total size of files in a directory.
fn calc_dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_deterministic() {
        let url = "https://www.youtube.com/watch?v=_g4l7YkDQwA";
        let key1 = VideoCache::cache_key(url);
        let key2 = VideoCache::cache_key(url);
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 64); // SHA256 hex = 64 chars
    }

    #[test]
    fn test_cache_key_normalization() {
        // YouTube URL variants should produce the same key
        let url1 = "https://youtu.be/_g4l7YkDQwA?si=fyWWkLYS_qhYPdd-";
        let url2 = "https://www.youtube.com/watch?v=_g4l7YkDQwA";
        let key1 = VideoCache::cache_key(url1);
        let key2 = VideoCache::cache_key(url2);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_strips_list_param() {
        let url1 = "https://www.youtube.com/watch?v=abc123&list=PLxxx";
        let url2 = "https://www.youtube.com/watch?v=abc123";
        let key1 = VideoCache::cache_key(url1);
        let key2 = VideoCache::cache_key(url2);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_normalize_url() {
        assert_eq!(
            normalize_url("https://youtu.be/abc?si=xyz"),
            "https://www.youtube.com/watch?v=abc"
        );
        assert_eq!(
            normalize_url("https://www.youtube.com/watch?v=abc&t=10"),
            "https://www.youtube.com/watch?v=abc&t=10"
        );
        assert_eq!(
            normalize_url("https://example.com/video.mp4"),
            "https://example.com/video.mp4"
        );
    }

    #[test]
    fn test_cache_store_and_retrieve_info() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cache = VideoCache::with_dir(tmp.path().to_path_buf()).unwrap();

        let url = "https://www.youtube.com/watch?v=test123";
        let info = VideoInfo {
            title: "Test Video".to_string(),
            uploader: Some("Test Channel".to_string()),
            duration: Some(120.0),
            language: Some("en".to_string()),
            description: None,
        };

        cache.store_info(url, &info).unwrap();
        let retrieved = cache.get_info(url).unwrap();
        assert_eq!(retrieved.title, "Test Video");
        assert_eq!(retrieved.duration, Some(120.0));
    }

    #[test]
    fn test_cache_invalidation() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cache = VideoCache::with_dir(tmp.path().to_path_buf()).unwrap();

        let url = "https://www.youtube.com/watch?v=test456";
        let info = VideoInfo::default();
        cache.store_info(url, &info).unwrap();
        assert!(cache.get_info(url).is_some());

        cache.invalidate(url).unwrap();
        assert!(cache.get_info(url).is_none());
    }

    #[test]
    fn test_cache_stats() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = VideoCache::with_dir(tmp.path().to_path_buf()).unwrap();
        assert_eq!(cache.entry_count(), 0);
        assert_eq!(cache.total_size(), 0);
    }

    #[test]
    fn test_cache_eviction() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cache = VideoCache::with_dir(tmp.path().to_path_buf()).unwrap();
        // Set very small max size to trigger eviction
        cache.max_size_bytes = 100;

        let url = "https://www.youtube.com/watch?v=test789";
        let info = VideoInfo::default();
        cache.store_info(url, &info).unwrap();
        assert!(cache.entry_count() > 0);

        // Evict should remove entries since we're over the tiny limit
        let evicted = cache.evict().unwrap();
        assert!(!evicted.is_empty());
    }
}
