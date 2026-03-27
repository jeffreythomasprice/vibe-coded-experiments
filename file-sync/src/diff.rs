use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use filetime::FileTime;

#[derive(Debug)]
pub struct FileCopy {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: SystemTime,
}

impl FileCopy {
    pub fn from_path(path: &Path) -> Result<Self> {
        let meta = fs::metadata(path)
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
        let ft = FileTime::from_last_modification_time(&meta);
        let mtime = SystemTime::UNIX_EPOCH
            + std::time::Duration::new(ft.unix_seconds() as u64, ft.nanoseconds());
        Ok(FileCopy {
            path: path.to_path_buf(),
            size: meta.len(),
            mtime,
        })
    }

    pub fn mtime_string(&self) -> String {
        let duration = self
            .mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = duration.as_secs();
        // Simple UTC formatting
        let days = secs / 86400;
        let time_secs = secs % 86400;
        let hours = time_secs / 3600;
        let minutes = (time_secs % 3600) / 60;

        // Calculate date from days since epoch
        let (year, month, day) = days_to_date(days);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            year, month, day, hours, minutes
        )
    }
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[derive(Debug)]
pub struct ConflictInfo {
    pub entity_name: String,
    pub copies: Vec<FileCopy>,
    pub is_binary: bool,
    pub texts: Vec<Option<String>>,
    pub group_sizes: Vec<usize>,
}

pub fn build_conflict_info_multi(
    entity_name: &str,
    paths: &[&Path],
    contents: &[&[u8]],
    group_sizes: &[usize],
) -> Result<ConflictInfo> {
    let copies: Vec<FileCopy> = paths
        .iter()
        .map(|p| FileCopy::from_path(p))
        .collect::<Result<Vec<_>>>()?;

    let binary = contents.iter().any(|data| {
        let check_len = data.len().min(8192);
        data[..check_len].contains(&0)
    });

    let texts = if !binary {
        contents
            .iter()
            .map(|data| std::str::from_utf8(data).ok().map(|s| s.to_string()))
            .collect()
    } else {
        vec![None; paths.len()]
    };

    Ok(ConflictInfo {
        entity_name: entity_name.to_string(),
        copies,
        is_binary: binary,
        texts,
        group_sizes: group_sizes.to_vec(),
    })
}

