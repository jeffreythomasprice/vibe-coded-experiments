//! A tiny disk cache for provider responses that are expensive or
//! rate-limited to refetch.
//!
//! The one consumer that actually needs this is `llm::ollama::library`'s
//! scrape of `ollama.com/library`: there is no JSON API behind it (see that
//! module's doc), so re-fetching an ~800KB HTML page on every call is not
//! acceptable, and a per-model tags page is a second request on top of that.
//! The three JSON model-listing endpoints (`ModelProvider::list_models` on
//! all three providers) are individually cheap enough to always hit the
//! network, but `catalog::load` wires a `Cache` into all three anyway —
//! calling all of them together on every launch is what wants caching, even
//! though no single one of them would.
//!
//! Stores the *raw response body* — the JSON text or the HTML — not a parsed
//! struct, so the bytes on disk are the same shape as a checked-in fixture
//! and changing a parser never requires invalidating the cache. Freshness is
//! decided by file mtime against an injected `now`, the same trick
//! `logging::rotate::RotatingWriter` uses for testability.
//!
//! Every failure here — an unreadable directory, a missing mtime, a failed
//! write — is logged at `debug` and treated as a cache miss or a no-op
//! write. A broken cache must never break a model listing.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::config::CacheConfig;

/// A disk cache keyed by a short string (see [`sanitize_key`]).
///
/// `Cache::disabled()` is the default for a client that hasn't opted into
/// caching via `with_cache` — it never hits or stores, so existing
/// construction sites behave exactly as before.
pub struct Cache {
    dir: Option<PathBuf>,
    ttl: Duration,
}

impl Cache {
    pub fn new(cfg: &CacheConfig) -> Self {
        Self {
            dir: Some(cfg.dir.clone()),
            ttl: Duration::from_secs(cfg.ttl_secs),
        }
    }

    pub fn disabled() -> Self {
        Self {
            dir: None,
            ttl: Duration::ZERO,
        }
    }

    /// Read `key` back if it exists and is younger than the configured TTL.
    /// Any failure along the way — no cache dir configured, the file is
    /// missing, its mtime can't be read, a zero TTL — is a plain `None`, not
    /// an error.
    pub fn read(&self, key: &str, now: SystemTime) -> Option<String> {
        let dir = self.dir.as_ref()?;
        let path = dir.join(sanitize_key(key));

        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(err) => {
                tracing::debug!(key, %err, "cache miss: could not stat cached entry");
                return None;
            }
        };
        let modified = match metadata.modified() {
            Ok(m) => m,
            Err(err) => {
                tracing::debug!(key, %err, "cache miss: no mtime available");
                return None;
            }
        };
        if !is_fresh(modified, now, self.ttl) {
            return None;
        }
        match std::fs::read_to_string(&path) {
            Ok(body) => Some(body),
            Err(err) => {
                tracing::debug!(key, %err, "cache miss: could not read cached entry");
                None
            }
        }
    }

    /// Best-effort write: a failure here (unwritable dir, disk full, ...) is
    /// logged and otherwise ignored — the caller already has the response in
    /// hand and doesn't need the write to succeed.
    pub fn write(&self, key: &str, body: &str) {
        let Some(dir) = &self.dir else { return };
        if let Err(err) = std::fs::create_dir_all(dir) {
            tracing::debug!(key, %err, "cache write skipped: could not create cache dir");
            return;
        }
        let path = dir.join(sanitize_key(key));
        if let Err(err) = std::fs::write(&path, body) {
            tracing::debug!(key, %err, "cache write failed");
        }
    }
}

/// Whether a cache entry last modified at `modified` is still fresh at
/// `now`, given `ttl`. A zero TTL is never fresh — that's how `[cache]`'s
/// `ttl_secs = 0` disables caching entirely. `modified` in the future (clock
/// skew, a restored backup) is treated as fresh rather than as an error.
pub fn is_fresh(modified: SystemTime, now: SystemTime, ttl: Duration) -> bool {
    if ttl.is_zero() {
        return false;
    }
    match now.duration_since(modified) {
        Ok(age) => age < ttl,
        Err(_) => true,
    }
}

/// Reduce a cache key to characters safe as a single path component, so a
/// caller-supplied value — `list_remote_tags`' model name comes from the
/// caller — can never escape the cache directory. `/` and `\` become `_`;
/// any other non `[A-Za-z0-9._-]` character becomes `_`; any resulting `..`
/// is collapsed so the sanitized string can never be interpreted as a
/// parent-directory path component once joined onto the cache dir.
pub fn sanitize_key(key: &str) -> String {
    let mut out: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    while out.contains("..") {
        out = out.replace("..", "__");
    }
    if out.is_empty() || out == "." {
        out = "_".to_string();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "ai-harness-cache-test-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }
        fn path(&self) -> PathBuf {
            self.0.clone()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn cfg(dir: PathBuf, ttl_secs: u64) -> CacheConfig {
        CacheConfig { dir, ttl_secs }
    }

    #[test]
    fn a_fresh_entry_is_younger_than_the_ttl() {
        let now = SystemTime::now();
        let modified = now - Duration::from_secs(10);
        assert!(is_fresh(modified, now, Duration::from_secs(60)));
    }

    #[test]
    fn an_entry_older_than_the_ttl_is_not_fresh() {
        let now = SystemTime::now();
        let modified = now - Duration::from_secs(120);
        assert!(!is_fresh(modified, now, Duration::from_secs(60)));
    }

    #[test]
    fn exactly_at_the_ttl_boundary_is_not_fresh() {
        let now = SystemTime::now();
        let ttl = Duration::from_secs(60);
        let modified = now - ttl;
        assert!(!is_fresh(modified, now, ttl));
    }

    #[test]
    fn a_zero_ttl_is_never_fresh() {
        let now = SystemTime::now();
        assert!(!is_fresh(now, now, Duration::ZERO));
    }

    #[test]
    fn future_mtime_from_clock_skew_is_treated_as_fresh() {
        let now = SystemTime::now();
        let modified = now + Duration::from_secs(10);
        assert!(is_fresh(modified, now, Duration::from_secs(60)));
    }

    #[test]
    fn sanitize_key_passes_through_ordinary_names() {
        assert_eq!(sanitize_key("ollama-tags.json"), "ollama-tags.json");
        assert_eq!(sanitize_key("llama3.1"), "llama3.1");
    }

    #[test]
    fn sanitize_key_strips_path_separators() {
        assert_eq!(sanitize_key("a/b\\c"), "a_b_c");
    }

    #[test]
    fn sanitize_key_neutralizes_parent_directory_traversal() {
        let sanitized = sanitize_key("../../etc/passwd");
        assert!(!sanitized.contains(".."), "still contains .. : {sanitized}");
        assert!(!sanitized.contains('/'), "still contains / : {sanitized}");
    }

    #[test]
    fn sanitize_key_never_produces_an_empty_or_dot_only_component() {
        assert_eq!(sanitize_key(""), "_");
        assert_eq!(sanitize_key("."), "_");
    }

    #[test]
    fn write_then_read_round_trips() {
        let dir = TempDir::new("roundtrip");
        let cache = Cache::new(&cfg(dir.path(), 3600));
        cache.write("key.json", "hello");
        let now = SystemTime::now();
        assert_eq!(cache.read("key.json", now).as_deref(), Some("hello"));
    }

    #[test]
    fn a_missing_entry_reads_as_none_not_an_error() {
        let dir = TempDir::new("missing");
        let cache = Cache::new(&cfg(dir.path(), 3600));
        assert_eq!(cache.read("nope.json", SystemTime::now()), None);
    }

    #[test]
    fn a_missing_cache_directory_reads_as_none_not_an_error() {
        let dir = TempDir::new("nodir");
        // Deliberately never created.
        let cache = Cache::new(&cfg(dir.path(), 3600));
        assert_eq!(cache.read("key.json", SystemTime::now()), None);
    }

    #[test]
    fn a_disabled_cache_never_reads_or_writes() {
        let dir = TempDir::new("disabled");
        let cache = Cache::disabled();
        cache.write("key.json", "hello");
        assert_eq!(cache.read("key.json", SystemTime::now()), None);
        assert!(!dir.path().exists(), "disabled cache must not create a directory");
    }

    #[test]
    fn a_stale_entry_on_disk_is_not_returned() {
        let dir = TempDir::new("stale");
        let cache = Cache::new(&cfg(dir.path(), 60));
        cache.write("key.json", "hello");
        let far_future = SystemTime::now() + Duration::from_secs(3600);
        assert_eq!(cache.read("key.json", far_future), None);
    }
}
