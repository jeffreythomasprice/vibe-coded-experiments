//! Discovers every crate in this Cargo workspace at build time and bakes their
//! tracing target names into `WORKSPACE_CRATE_TARGETS` (a comma-separated list).
//!
//! Doing this here — rather than hardcoding crate names in the logging setup —
//! means new crates added to the workspace are automatically treated as "our"
//! code by the default log filter without touching any source.

use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is always set by cargo"),
    );

    let workspace_root = find_workspace_root(&manifest_dir)
        .expect("could not locate a Cargo.toml containing a [workspace] table");
    let root_manifest = workspace_root.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", root_manifest.display());

    let mut targets = Vec::new();
    for member_manifest in member_manifests(&workspace_root, &root_manifest) {
        println!("cargo:rerun-if-changed={}", member_manifest.display());
        if let Some(name) = package_name(&member_manifest) {
            // tracing targets use the crate's module path, where `-` becomes `_`.
            targets.push(name.replace('-', "_"));
        }
    }

    targets.sort();
    targets.dedup();
    println!(
        "cargo:rustc-env=WORKSPACE_CRATE_TARGETS={}",
        targets.join(",")
    );
}

/// Walk up from `start` until we find a `Cargo.toml` declaring `[workspace]`.
fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let manifest = dir.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            if let Ok(value) = text.parse::<toml::Value>() {
                if value.get("workspace").is_some() {
                    return Some(dir.to_path_buf());
                }
            }
        }
    }
    None
}

/// Resolve `[workspace] members` (expanding glob patterns like `crates/*`) into
/// the set of member `Cargo.toml` paths.
fn member_manifests(workspace_root: &Path, root_manifest: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(root_manifest) else {
        return Vec::new();
    };
    let Ok(value) = text.parse::<toml::Value>() else {
        return Vec::new();
    };

    let members = value
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();

    let mut manifests = Vec::new();
    for member in members {
        let pattern = workspace_root.join(member);
        if member.contains('*') {
            let Some(pattern) = pattern.to_str() else {
                continue;
            };
            if let Ok(paths) = glob::glob(pattern) {
                for dir in paths.flatten() {
                    let manifest = dir.join("Cargo.toml");
                    if manifest.is_file() {
                        manifests.push(manifest);
                    }
                }
            }
        } else {
            let manifest = pattern.join("Cargo.toml");
            if manifest.is_file() {
                manifests.push(manifest);
            }
        }
    }
    manifests
}

/// Read `[package] name` from a member manifest, if present (virtual manifests
/// and unparsable files are skipped).
fn package_name(manifest: &Path) -> Option<String> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let value = text.parse::<toml::Value>().ok()?;
    value
        .get("package")?
        .get("name")?
        .as_str()
        .map(String::from)
}
