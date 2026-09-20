use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const API_BASE: &str = "https://huggingface.co/api/models";
const CACHE_DIR: &str = ".hub-cache";
const CACHE_VERSION: u32 = 2;
const MAX_FETCH_THREADS: usize = 8;

#[derive(Debug, Clone)]
pub struct RepoSummary {
    pub id: String,
    pub downloads: u64,
}

#[derive(Debug, Clone)]
pub struct RepoFile {
    pub name: String,
    pub size: u64,
}

/// Whether a repo requires accepting a license (and possibly manual approval) before
/// its files can be downloaded. Downloading a gated repo without an authenticated,
/// access-granted request returns HTTP 401.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gated {
    Open,
    Auto,
    Manual,
}

impl Gated {
    pub fn as_str(self) -> &'static str {
        match self {
            Gated::Open => "-",
            Gated::Auto => "gated",
            Gated::Manual => "gated (manual)",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RepoInfo {
    pub gated: Gated,
    pub files: Vec<RepoFile>,
}

/// Mirrors HuggingFace's `gated` field, which is JSON `false` for an open repo or a
/// string ("auto"/"manual") for a gated one.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawGated {
    Flag(#[allow(dead_code)] bool),
    Kind(String),
}

impl Default for RawGated {
    fn default() -> Self {
        RawGated::Flag(false)
    }
}

impl From<RawGated> for Gated {
    fn from(raw: RawGated) -> Self {
        match raw {
            RawGated::Flag(_) => Gated::Open,
            RawGated::Kind(kind) if kind == "manual" => Gated::Manual,
            RawGated::Kind(_) => Gated::Auto,
        }
    }
}

#[derive(Debug, Error)]
pub enum HubError {
    #[error("huggingface request to {url} failed: {source}")]
    Request {
        url: String,
        #[source]
        source: Box<ureq::Error>,
    },

    #[error("huggingface returned {status} for {url}")]
    Status { status: u16, url: String },

    #[error("failed to parse the huggingface response from {url}: {source}")]
    Decode {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read the response cache at {}: {source}", .path.display())]
    CacheRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write the response cache at {}: {source}", .path.display())]
    CacheWrite {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("huggingface returned an unusable model id: {0}")]
    InvalidRepoId(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawSummary {
    id: String,
    #[serde(default)]
    downloads: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchEntry {
    version: u32,
    query: String,
    limit: u32,
    all: bool,
    results: Vec<RawSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawFile {
    #[serde(rename = "rfilename")]
    name: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct RawRepoInfo {
    #[serde(default)]
    gated: RawGated,
    #[serde(default)]
    siblings: Vec<RawFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BlobsEntry {
    version: u32,
    repo: String,
    gated: Gated,
    files: Vec<RawFile>,
}

/// Search HuggingFace for models matching `query`, returning up to `limit` results
/// sorted by downloads. Results are cached under `models_dir/.hub-cache`; `refresh`
/// ignores that cache and re-fetches.
pub fn search(
    models_dir: &Path,
    query: &str,
    limit: u32,
    all: bool,
    refresh: bool,
) -> Result<Vec<RepoSummary>, HubError> {
    let cache_dir = models_dir.join(CACHE_DIR);
    let cache_path = cache_dir.join(format!("search-{:016x}.json", search_cache_key(query, limit, all)));

    if !refresh
        && let Some(entry) = read_cache::<SearchEntry>(&cache_path)?
        && entry.query == query
        && entry.limit == limit
        && entry.all == all
    {
        return Ok(entry.results.into_iter().map(RepoSummary::from).collect());
    }

    let agent = ureq::Agent::new();
    let mut request = agent
        .get(API_BASE)
        .query("search", query)
        .query("sort", "downloads")
        .query("direction", "-1")
        .query("limit", &limit.to_string());
    if !all {
        request = request.query("filter", "text-to-image");
    }
    let url = request.url().to_owned();

    let results: Vec<RawSummary> = call_json(&url, request)?;

    write_cache(
        &cache_path,
        &SearchEntry {
            version: CACHE_VERSION,
            query: query.to_owned(),
            limit,
            all,
            results: results.clone(),
        },
    )?;

    Ok(results.into_iter().map(RepoSummary::from).collect())
}

/// Gated status and blob listing (file name + size) for each of `repos`, fetched in
/// parallel and cached per repo. Returns one entry per input repo, in the same order;
/// a repo whose request fails yields `None` rather than failing the whole batch.
pub fn files_for(models_dir: &Path, repos: &[String], refresh: bool) -> Vec<Option<RepoInfo>> {
    if repos.is_empty() {
        return Vec::new();
    }

    let cache_dir = models_dir.join(CACHE_DIR);
    let agent = ureq::Agent::new();
    let thread_count = MAX_FETCH_THREADS.min(repos.len()).max(1);

    let mut results: Vec<Option<RepoInfo>> = (0..repos.len()).map(|_| None).collect();
    let chunks: Vec<Vec<usize>> = (0..repos.len())
        .collect::<Vec<_>>()
        .chunks(repos.len().div_ceil(thread_count))
        .map(<[usize]>::to_vec)
        .collect();

    std::thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|indices| {
                let agent = agent.clone();
                let cache_dir = &cache_dir;
                scope.spawn(move || {
                    indices
                        .into_iter()
                        .map(|index| {
                            (
                                index,
                                fetch_repo_info(&agent, cache_dir, &repos[index], refresh),
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        for handle in handles {
            for (index, outcome) in handle.join().unwrap_or_default() {
                match outcome {
                    Ok(info) => results[index] = Some(info),
                    Err(err) => {
                        tracing::warn!(repo = %repos[index], error = %err, "failed to fetch file listing");
                    }
                }
            }
        }
    });

    results
}

fn fetch_repo_info(
    agent: &ureq::Agent,
    cache_dir: &Path,
    repo: &str,
    refresh: bool,
) -> Result<RepoInfo, HubError> {
    if repo.contains('/') && repo.matches('/').count() != 1 || repo.starts_with('.') {
        return Err(HubError::InvalidRepoId(repo.to_owned()));
    }

    let cache_path = cache_dir.join(format!("blobs-{}.json", repo.replace('/', "--")));

    if !refresh
        && let Some(entry) = read_cache::<BlobsEntry>(&cache_path)?
        && entry.repo == repo
    {
        return Ok(RepoInfo {
            gated: entry.gated,
            files: entry.files.into_iter().map(RepoFile::from).collect(),
        });
    }

    let url = format!("{API_BASE}/{repo}?blobs=true");
    let info: RawRepoInfo = call_json(&url, agent.get(&url))?;
    let gated = Gated::from(info.gated);

    write_cache(
        &cache_path,
        &BlobsEntry {
            version: CACHE_VERSION,
            repo: repo.to_owned(),
            gated,
            files: info.siblings.clone(),
        },
    )?;

    Ok(RepoInfo {
        gated,
        files: info.siblings.into_iter().map(RepoFile::from).collect(),
    })
}

fn call_json<T: serde::de::DeserializeOwned>(url: &str, request: ureq::Request) -> Result<T, HubError> {
    let response = request.call().map_err(|source| match &source {
        ureq::Error::Status(status, _) => HubError::Status {
            status: *status,
            url: url.to_owned(),
        },
        ureq::Error::Transport(_) => HubError::Request {
            url: url.to_owned(),
            source: Box::new(source),
        },
    })?;
    response.into_json().map_err(|source| HubError::Decode {
        url: url.to_owned(),
        source,
    })
}

fn search_cache_key(query: &str, limit: u32, all: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    query.hash(&mut hasher);
    limit.hash(&mut hasher);
    all.hash(&mut hasher);
    hasher.finish()
}

fn read_cache<T: serde::de::DeserializeOwned + HasVersion>(path: &Path) -> Result<Option<T>, HubError> {
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<T>(&bytes) {
            Ok(entry) if entry.version() == CACHE_VERSION => Ok(Some(entry)),
            _ => Ok(None),
        },
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(HubError::CacheRead {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn write_cache<T: Serialize>(path: &Path, entry: &T) -> Result<(), HubError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).map_err(|source| HubError::CacheWrite {
        path: path.to_path_buf(),
        source,
    })?;

    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|source| HubError::CacheWrite {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::to_writer(&mut file, entry).map_err(|source| HubError::CacheWrite {
        path: path.to_path_buf(),
        source: source.into(),
    })?;
    file.persist(path).map_err(|err| HubError::CacheWrite {
        path: path.to_path_buf(),
        source: err.error,
    })?;

    Ok(())
}

trait HasVersion {
    fn version(&self) -> u32;
}

impl HasVersion for SearchEntry {
    fn version(&self) -> u32 {
        self.version
    }
}

impl HasVersion for BlobsEntry {
    fn version(&self) -> u32 {
        self.version
    }
}

impl From<RawSummary> for RepoSummary {
    fn from(raw: RawSummary) -> Self {
        Self {
            id: raw.id,
            downloads: raw.downloads,
        }
    }
}

impl From<RawFile> for RepoFile {
    fn from(raw: RawFile) -> Self {
        Self {
            name: raw.name,
            size: raw.size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_cache_key_is_stable() {
        let a = search_cache_key("turbo", 20, false);
        let b = search_cache_key("turbo", 20, false);
        assert_eq!(a, b);
    }

    #[test]
    fn search_cache_key_differs_by_query() {
        assert_ne!(
            search_cache_key("turbo", 20, false),
            search_cache_key("sdxl", 20, false)
        );
    }

    #[test]
    fn search_cache_key_differs_by_limit_and_all() {
        assert_ne!(
            search_cache_key("turbo", 20, false),
            search_cache_key("turbo", 50, false)
        );
        assert_ne!(
            search_cache_key("turbo", 20, false),
            search_cache_key("turbo", 20, true)
        );
    }

    #[test]
    fn raw_summary_deserializes_hf_response_shape() {
        let json = r#"[{"_id":"x","id":"stabilityai/sd-turbo","likes":462,"downloads":339377}]"#;
        let parsed: Vec<RawSummary> = serde_json::from_str(json).unwrap();
        assert_eq!(parsed[0].id, "stabilityai/sd-turbo");
        assert_eq!(parsed[0].downloads, 339377);
    }

    #[test]
    fn raw_repo_info_deserializes_blob_sizes() {
        let json = r#"{"gated":false,"siblings":[{"rfilename":"sd_turbo.safetensors","size":5214561328}]}"#;
        let parsed: RawRepoInfo = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.siblings[0].name, "sd_turbo.safetensors");
        assert_eq!(parsed.siblings[0].size, 5214561328);
        assert_eq!(Gated::from(parsed.gated), Gated::Open);
    }

    #[test]
    fn raw_repo_info_deserializes_gated_kinds() {
        let auto: RawRepoInfo = serde_json::from_str(r#"{"gated":"auto","siblings":[]}"#).unwrap();
        assert_eq!(Gated::from(auto.gated), Gated::Auto);

        let manual: RawRepoInfo =
            serde_json::from_str(r#"{"gated":"manual","siblings":[]}"#).unwrap();
        assert_eq!(Gated::from(manual.gated), Gated::Manual);
    }

    #[test]
    fn fetch_repo_info_rejects_unusable_repo_ids() {
        let dir = tempfile::tempdir().unwrap();
        let agent = ureq::Agent::new();
        assert!(matches!(
            fetch_repo_info(&agent, dir.path(), "../escape", false),
            Err(HubError::InvalidRepoId(_))
        ));
        assert!(matches!(
            fetch_repo_info(&agent, dir.path(), "owner/repo/extra", false),
            Err(HubError::InvalidRepoId(_))
        ));
    }

    #[test]
    fn cache_round_trips_search_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("search-abc.json");
        let entry = SearchEntry {
            version: CACHE_VERSION,
            query: "turbo".to_owned(),
            limit: 20,
            all: false,
            results: vec![RawSummary {
                id: "stabilityai/sd-turbo".to_owned(),
                downloads: 339377,
            }],
        };
        write_cache(&path, &entry).unwrap();

        let read: SearchEntry = read_cache(&path).unwrap().unwrap();
        assert_eq!(read.query, "turbo");
        assert_eq!(read.results[0].id, "stabilityai/sd-turbo");
    }

    #[test]
    fn cache_rejects_wrong_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("search-old.json");
        std::fs::write(
            &path,
            r#"{"version":999,"query":"turbo","limit":20,"all":false,"results":[]}"#,
        )
        .unwrap();

        let read: Option<SearchEntry> = read_cache(&path).unwrap();
        assert!(read.is_none());
    }

    #[test]
    fn cache_miss_on_missing_file_is_none_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.json");
        let read: Option<SearchEntry> = read_cache(&missing).unwrap();
        assert!(read.is_none());
    }

    #[test]
    fn files_for_empty_input_does_no_work() {
        let dir = tempfile::tempdir().unwrap();
        assert!(files_for(dir.path(), &[], false).is_empty());
    }
}
