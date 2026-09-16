//! Bakes build-time config into the wasm bundle as `env!(...)` constants (see `src/config.rs`).
//! Layers `.env` < `.env.<profile>` < `.env.local`, later files winning key-for-key -- `<profile>`
//! is `development` for a debug build, `production` for a release build (so `trunk serve` and
//! `trunk build --release`/`deploy.sh` pick the right API host with no flag). Never touches the
//! process environment itself (`dotenvy::from_path_iter` only parses), since only the values that
//! win the layering ever need to exist anywhere.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const REQUIRED_KEYS: &[&str] = &["API_BASE_URL"];

#[derive(Debug, thiserror::Error)]
enum EnvError {
    #[error("could not read {path}: {source}")]
    Read { path: PathBuf, source: dotenvy::Error },
    #[error("{key} is not set -- add it to client/.env, client/.env.{profile}, or client/.env.local")]
    MissingKey { key: &'static str, profile: &'static str },
}

fn layer(path: &Path, values: &mut BTreeMap<String, String>) -> Result<(), EnvError> {
    match dotenvy::from_path_iter(path) {
        Ok(iter) => {
            for item in iter {
                let (key, value) = item.map_err(|source| EnvError::Read { path: path.to_path_buf(), source })?;
                values.insert(key, value);
            }
            Ok(())
        }
        Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(EnvError::Read { path: path.to_path_buf(), source }),
    }
}

fn main() -> Result<(), EnvError> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let profile = if std::env::var("PROFILE").as_deref() == Ok("release") { "production" } else { "development" };

    let layers =
        [manifest_dir.join(".env"), manifest_dir.join(format!(".env.{profile}")), manifest_dir.join(".env.local")];

    let mut values = BTreeMap::new();
    for path in &layers {
        // Unconditional, even for a file that doesn't exist yet: creating `.env.local` later must
        // still trigger a rebuild, and cargo only recompiles when the emitted `rustc-env` values
        // actually change, so this costs nothing when nothing changed.
        println!("cargo::rerun-if-changed={}", path.display());
        layer(path, &mut values)?;
    }

    for key in REQUIRED_KEYS {
        let value = values.get(*key).ok_or(EnvError::MissingKey { key, profile })?;
        println!("cargo::rustc-env={key}={value}");
    }

    Ok(())
}
