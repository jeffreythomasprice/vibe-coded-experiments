use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::sd::SdError;
use crate::sd::config::{ReleaseBackend, SdConfig};

const GITHUB_API_BASE: &str = "https://api.github.com/repos/leejet/stable-diffusion.cpp";
const BINARY_NAME: &str = "sd-server";
const USER_AGENT: &str = "image-gen";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Ensures a `sd-server` binary matching `config.backend` is present on disk for
/// `config.release_tag`, downloading and extracting it if necessary, and returns
/// its path. Idempotent: a literal (non-`"latest"`) tag that is already
/// extracted is returned immediately with no network access at all; `"latest"`
/// always resolves against the GitHub API first, since what it means can change
/// between calls.
pub async fn ensure_release(client: &reqwest::Client, config: &SdConfig) -> Result<PathBuf, SdError> {
    ensure_release_from(client, GITHUB_API_BASE, config).await
}

async fn ensure_release_from(
    client: &reqwest::Client,
    api_base: &str,
    config: &SdConfig,
) -> Result<PathBuf, SdError> {
    if config.release_tag != "latest" {
        let binary = server_binary_path(&config.server_dir, &config.release_tag);
        if binary.is_file() {
            return Ok(binary);
        }
    }

    let release = fetch_release(client, api_base, &config.release_tag).await?;
    let binary = server_binary_path(&config.server_dir, &release.tag_name);
    if binary.is_file() {
        return Ok(binary);
    }

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let asset = select_asset(&release.assets, os, arch, config.backend).ok_or_else(|| {
        SdError::NoAssetForPlatform {
            os: os.to_owned(),
            arch: arch.to_owned(),
            backend: config.backend.as_str().to_owned(),
        }
    })?;

    let bytes = download_asset(client, asset).await?;
    let dest_dir = config.server_dir.join(&release.tag_name);
    extract_zip(asset.name.clone(), bytes, dest_dir).await?;

    if !binary.is_file() {
        return Err(SdError::MissingBinary(binary));
    }
    Ok(binary)
}

fn server_binary_path(server_dir: &Path, tag: &str) -> PathBuf {
    server_dir.join(tag).join(BINARY_NAME)
}

async fn fetch_release(client: &reqwest::Client, api_base: &str, tag: &str) -> Result<GithubRelease, SdError> {
    let url = if tag == "latest" {
        format!("{api_base}/releases/latest")
    } else {
        format!("{api_base}/releases/tags/{tag}")
    };

    let response = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|source| SdError::Request {
            url: url.clone(),
            source: Box::new(source),
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|source| SdError::Request {
        url: url.clone(),
        source: Box::new(source),
    })?;
    if !status.is_success() {
        return Err(SdError::Status {
            status: status.as_u16(),
            url,
            body,
        });
    }

    serde_json::from_str(&body).map_err(|source| SdError::Decode { url, source })
}

/// Picks the release asset matching `os`/`arch`/`backend` out of a release's
/// full asset list, rather than constructing an expected filename directly —
/// upstream's names carry extra, version-specific segments (e.g. a ROCm
/// version suffix) that would make exact construction brittle.
fn select_asset<'a>(
    assets: &'a [GithubAsset],
    os: &str,
    arch: &str,
    backend: ReleaseBackend,
) -> Option<&'a GithubAsset> {
    assets.iter().find(|asset| matches_asset(&asset.name, os, arch, backend))
}

fn matches_asset(name: &str, os: &str, arch: &str, backend: ReleaseBackend) -> bool {
    if !name.ends_with(".zip") {
        return false;
    }
    let os_tag = match os {
        "linux" => "-Linux-",
        "macos" => "-Darwin-",
        "windows" => "-win-",
        _ => return false,
    };
    if !name.contains(os_tag) {
        return false;
    }
    let arch_ok = match (os, arch) {
        ("linux", "x86_64") | ("windows", "x86_64") => name.contains("x86_64"),
        ("macos", "aarch64") => name.contains("arm64"),
        _ => false,
    };
    if !arch_ok {
        return false;
    }
    match backend {
        ReleaseBackend::Vulkan => name.contains("-vulkan"),
        ReleaseBackend::Rocm => name.contains("-rocm"),
        ReleaseBackend::Cpu => !name.contains("-vulkan") && !name.contains("-rocm") && !name.contains("-cuda"),
    }
}

async fn download_asset(client: &reqwest::Client, asset: &GithubAsset) -> Result<Vec<u8>, SdError> {
    let fail = |source: reqwest::Error| SdError::Download {
        url: asset.browser_download_url.clone(),
        source: Box::new(source),
    };

    let response = client
        .get(&asset.browser_download_url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(fail)?;
    let response = response.error_for_status().map_err(fail)?;
    let bytes = response.bytes().await.map_err(fail)?.to_vec();

    if bytes.len() as u64 != asset.size {
        return Err(SdError::SizeMismatch {
            url: asset.browser_download_url.clone(),
            expected: asset.size,
            actual: bytes.len() as u64,
        });
    }
    Ok(bytes)
}

/// Extraction is synchronous (the `zip` crate) and CPU/IO-bound, so it runs on
/// a blocking-pool thread rather than the async worker running the download.
async fn extract_zip(asset_name: String, bytes: Vec<u8>, dest_dir: PathBuf) -> Result<(), SdError> {
    tokio::task::spawn_blocking(move || extract_zip_blocking(&asset_name, &bytes, &dest_dir))
        .await
        .expect("extraction task panicked")
}

fn extract_zip_blocking(asset_name: &str, bytes: &[u8], dest_dir: &Path) -> Result<(), SdError> {
    let fail_extract = |source: zip::result::ZipError| SdError::Extract {
        asset: asset_name.to_owned(),
        source: Box::new(source),
    };

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(fail_extract)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(fail_extract)?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dest_dir.join(relative);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|source| io_err("create directory", &out_path, source))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| io_err("create directory", parent, source))?;
        }
        let mut out_file =
            std::fs::File::create(&out_path).map_err(|source| io_err("create", &out_path, source))?;
        std::io::copy(&mut entry, &mut out_file).map_err(|source| io_err("write", &out_path, source))?;

        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode))
                .map_err(|source| io_err("set permissions on", &out_path, source))?;
        }
    }

    Ok(())
}

fn io_err(action: &'static str, path: &Path, source: std::io::Error) -> SdError {
    SdError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const LINUX_ASSETS: &[(&str, &str)] = &[
        ("sd-master-890-74988b2-bin-Linux-Ubuntu-24.04-x86_64.zip", "cpu"),
        (
            "sd-master-890-74988b2-bin-Linux-Ubuntu-24.04-x86_64-vulkan.zip",
            "vulkan",
        ),
        (
            "sd-master-890-74988b2-bin-Linux-Ubuntu-24.04-x86_64-rocm-7.14.0.zip",
            "rocm",
        ),
        ("sd-master-890-74988b2-bin-Darwin-macOS-26.6.2-arm64.zip", "-"),
        ("sd-master-890-74988b2-bin-win-cuda12-x64.zip", "-"),
        ("cudart-sd-bin-win-cu12-x64.zip", "-"),
    ];

    fn linux_assets() -> Vec<GithubAsset> {
        LINUX_ASSETS
            .iter()
            .map(|(name, _)| GithubAsset {
                name: (*name).to_owned(),
                browser_download_url: format!("https://example.invalid/{name}"),
                size: 1,
            })
            .collect()
    }

    #[test]
    fn selects_plain_linux_asset_for_cpu() {
        let assets = linux_assets();
        let picked = select_asset(&assets, "linux", "x86_64", ReleaseBackend::Cpu).unwrap();
        assert_eq!(picked.name, "sd-master-890-74988b2-bin-Linux-Ubuntu-24.04-x86_64.zip");
    }

    #[test]
    fn selects_vulkan_linux_asset() {
        let assets = linux_assets();
        let picked = select_asset(&assets, "linux", "x86_64", ReleaseBackend::Vulkan).unwrap();
        assert_eq!(
            picked.name,
            "sd-master-890-74988b2-bin-Linux-Ubuntu-24.04-x86_64-vulkan.zip"
        );
    }

    #[test]
    fn selects_rocm_linux_asset_despite_version_suffix() {
        let assets = linux_assets();
        let picked = select_asset(&assets, "linux", "x86_64", ReleaseBackend::Rocm).unwrap();
        assert_eq!(
            picked.name,
            "sd-master-890-74988b2-bin-Linux-Ubuntu-24.04-x86_64-rocm-7.14.0.zip"
        );
    }

    #[test]
    fn no_linux_cuda_asset_exists() {
        // There is no Linux CUDA release upstream; this is why `ModelSet`'s
        // runtime `--backend cuda` must be rejected independently of asset
        // selection, not solved by finding an asset for it here.
        let assets = linux_assets();
        for backend in [ReleaseBackend::Cpu, ReleaseBackend::Vulkan, ReleaseBackend::Rocm] {
            let picked = select_asset(&assets, "linux", "x86_64", backend).unwrap();
            assert!(!picked.name.contains("cuda"));
        }
    }

    #[test]
    fn unknown_os_matches_nothing() {
        let assets = linux_assets();
        assert!(select_asset(&assets, "freebsd", "x86_64", ReleaseBackend::Cpu).is_none());
    }

    fn fake_zip(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            for (name, contents, mode) in entries {
                let options = zip::write::SimpleFileOptions::default().unix_permissions(*mode);
                writer.start_file(*name, options).unwrap();
                writer.write_all(contents).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    #[tokio::test]
    async fn extract_zip_preserves_the_executable_bit() {
        let dir = tempfile::tempdir().unwrap();
        let zip_bytes = fake_zip(&[
            (BINARY_NAME, b"fake server binary", 0o755),
            ("libggml.so", b"fake shared lib", 0o644),
        ]);

        extract_zip("test.zip".to_owned(), zip_bytes, dir.path().to_path_buf())
            .await
            .unwrap();

        let server_path = dir.path().join(BINARY_NAME);
        assert!(server_path.is_file());
        assert_eq!(std::fs::read(&server_path).unwrap(), b"fake server binary");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&server_path).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "sd-server should be executable");
        }
    }

    #[tokio::test]
    async fn extract_zip_preserves_nested_directories() {
        let dir = tempfile::tempdir().unwrap();
        let zip_bytes = fake_zip(&[("nested/dir/file.txt", b"hello", 0o644)]);

        extract_zip("test.zip".to_owned(), zip_bytes, dir.path().to_path_buf())
            .await
            .unwrap();

        assert_eq!(
            std::fs::read(dir.path().join("nested/dir/file.txt")).unwrap(),
            b"hello"
        );
    }

    #[tokio::test]
    async fn ensure_release_downloads_extracts_and_is_idempotent() {
        let server = MockServer::start().await;
        let zip_bytes = fake_zip(&[(BINARY_NAME, b"fake server binary", 0o755)]);

        Mock::given(method("GET"))
            .and(path("/releases/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tag_name": "master-890-74988b2",
                "assets": [{
                    "name": "sd-master-890-74988b2-bin-Linux-Ubuntu-24.04-x86_64-vulkan.zip",
                    "browser_download_url": format!("{}/download/sd.zip", server.uri()),
                    "size": zip_bytes.len(),
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/sd.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(zip_bytes))
            .expect(1)
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let config = SdConfig {
            backend: ReleaseBackend::Vulkan,
            release_tag: "latest".to_owned(),
            server_dir: dir.path().to_path_buf(),
            ..SdConfig::default()
        };
        let client = reqwest::Client::new();

        let binary = ensure_release_from(&client, &server.uri(), &config).await.unwrap();
        assert!(binary.is_file());
        assert_eq!(binary, dir.path().join("master-890-74988b2").join(BINARY_NAME));

        // Second call with the resolved (non-"latest") tag hits neither mock —
        // wiremock's `.expect(1)` on both routes fails the test on drop if it did.
        let pinned = SdConfig {
            release_tag: "master-890-74988b2".to_owned(),
            ..config
        };
        let binary_again = ensure_release_from(&client, &server.uri(), &pinned).await.unwrap();
        assert_eq!(binary, binary_again);
    }

    #[tokio::test]
    async fn missing_asset_for_platform_is_reported() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/releases/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tag_name": "master-890-74988b2",
                "assets": [{
                    "name": "sd-master-890-74988b2-bin-win-cuda12-x64.zip",
                    "browser_download_url": "https://example.invalid/x.zip",
                    "size": 1,
                }]
            })))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let config = SdConfig {
            backend: ReleaseBackend::Vulkan,
            release_tag: "latest".to_owned(),
            server_dir: dir.path().to_path_buf(),
            ..SdConfig::default()
        };
        let client = reqwest::Client::new();

        let err = ensure_release_from(&client, &server.uri(), &config).await.unwrap_err();
        assert!(matches!(err, SdError::NoAssetForPlatform { .. }));
    }

    #[tokio::test]
    async fn release_not_found_is_a_status_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/releases/tags/nonexistent"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let config = SdConfig {
            release_tag: "nonexistent".to_owned(),
            server_dir: dir.path().to_path_buf(),
            ..SdConfig::default()
        };
        let client = reqwest::Client::new();

        let err = ensure_release_from(&client, &server.uri(), &config).await.unwrap_err();
        assert!(matches!(err, SdError::Status { status: 404, .. }));
    }
}
