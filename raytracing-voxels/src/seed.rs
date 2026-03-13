use std::fs;
use std::path::Path;

use anyhow::Result;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SeedData {
    pub seed: u32,
}

pub fn load_seed(storage_dir: &Path) -> Result<Option<SeedData>> {
    let path = storage_dir.join("seed.json");
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)?;
    let data: SeedData = serde_json::from_str(&contents)?;
    Ok(Some(data))
}

pub fn save_seed(storage_dir: &Path, data: &SeedData) -> Result<()> {
    let path = storage_dir.join("seed.json");
    let tmp_path = storage_dir.join("seed.json.tmp");
    let json = serde_json::to_string_pretty(data)?;
    fs::write(&tmp_path, &json)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

pub fn resolve_seed(storage_dir: &Path, config_seed: Option<u32>) -> Result<u32> {
    if let Some(s) = config_seed {
        log::info!("using seed from config: {s}");
        save_seed(storage_dir, &SeedData { seed: s })?;
        return Ok(s);
    }

    if let Some(data) = load_seed(storage_dir)? {
        log::info!("loaded seed from file: {}", data.seed);
        return Ok(data.seed);
    }

    let seed: u32 = rand::rng().random();
    log::info!("generated new seed: {seed}");
    save_seed(storage_dir, &SeedData { seed })?;
    Ok(seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seed_test_{}", rand::rng().random::<u32>()));
        fs::create_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn resolve_generates_and_persists_new_seed() {
        let dir = temp_dir();
        let seed = resolve_seed(&dir, None).unwrap();
        let loaded = load_seed(&dir).unwrap().unwrap();
        assert_eq!(seed, loaded.seed);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_returns_persisted_seed() {
        let dir = temp_dir();
        save_seed(&dir, &SeedData { seed: 777 }).unwrap();
        let seed = resolve_seed(&dir, None).unwrap();
        assert_eq!(seed, 777);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_config_overrides_file() {
        let dir = temp_dir();
        save_seed(&dir, &SeedData { seed: 777 }).unwrap();
        let seed = resolve_seed(&dir, Some(999)).unwrap();
        assert_eq!(seed, 999);
        let loaded = load_seed(&dir).unwrap().unwrap();
        assert_eq!(loaded.seed, 999);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trip_save_load() {
        let dir = temp_dir();
        let data = SeedData { seed: 42 };
        save_seed(&dir, &data).unwrap();
        let loaded = load_seed(&dir).unwrap().unwrap();
        assert_eq!(loaded.seed, 42);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_twice_returns_same_seed() {
        let dir = temp_dir();
        let first = resolve_seed(&dir, None).unwrap();
        let second = resolve_seed(&dir, None).unwrap();
        assert_eq!(first, second);
        fs::remove_dir_all(&dir).ok();
    }
}
