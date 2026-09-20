use std::path::Path;

use crate::cli::{ListArgs, SearchArgs};
use crate::error::AppError;
use crate::hub;
use crate::models;

struct Row {
    status: String,
    access: String,
    size: Option<u64>,
    pulls: Option<u64>,
    model: String,
}

/// Search HuggingFace for models matching `args.query`, one output row per weight
/// file, marking rows already present under `models_dir`. The ACCESS column shows
/// whether the repo is gated: downloading a gated repo needs an authenticated,
/// access-granted request or it 401s.
pub fn search(args: &SearchArgs, models_dir: &Path) -> Result<(), AppError> {
    let summaries = hub::search(models_dir, &args.query, args.limit, args.all, args.refresh)?;
    let repo_ids: Vec<String> = summaries.iter().map(|s| s.id.clone()).collect();
    let infos = hub::files_for(models_dir, &repo_ids, args.refresh);

    let mut rows = Vec::new();
    for (summary, info) in summaries.iter().zip(infos) {
        let Some(info) = info else {
            rows.push(Row {
                status: "?".to_owned(),
                access: String::new(),
                size: None,
                pulls: Some(summary.downloads),
                model: summary.id.clone(),
            });
            continue;
        };

        let names: Vec<&str> = info.files.iter().map(|f| f.name.as_str()).collect();
        for name in models::weight_candidates(&names) {
            let size = info.files.iter().find(|f| f.name == name).map(|f| f.size);
            let status = if models::cached_path(models_dir, &summary.id, name).is_some() {
                "local"
            } else {
                "-"
            };
            rows.push(Row {
                status: status.to_owned(),
                access: info.gated.as_str().to_owned(),
                size,
                pulls: Some(summary.downloads),
                model: format!("{}:{name}", summary.id),
            });
        }
    }

    if rows.is_empty() {
        println!("no matching models found");
        return Ok(());
    }

    print_table(&rows, true);
    Ok(())
}

/// List every weight file already downloaded into `models_dir`, optionally filtered
/// to refs containing `args.query`. Touches no network.
pub fn list(args: &ListArgs, models_dir: &Path) -> Result<(), AppError> {
    let mut weights = models::cached_weights(models_dir)?;
    weights.sort_by(|a, b| (&a.repo, &a.file).cmp(&(&b.repo, &b.file)));

    let rows: Vec<Row> = weights
        .into_iter()
        .map(|w| Row {
            status: String::new(),
            access: String::new(),
            size: Some(w.size),
            pulls: None,
            model: format!("{}:{}", w.repo, w.file),
        })
        .filter(|row| match &args.query {
            Some(query) => row.model.contains(query.as_str()),
            None => true,
        })
        .collect();

    if rows.is_empty() {
        println!("no cached models found in {}", models_dir.display());
        return Ok(());
    }

    let total: u64 = rows.iter().filter_map(|row| row.size).sum();
    print_table(&rows, false);
    println!("{} total in {}", format_size(total), models_dir.display());
    Ok(())
}

fn print_table(rows: &[Row], show_status_and_pulls: bool) {
    let size_strs: Vec<String> = rows
        .iter()
        .map(|row| row.size.map_or_else(|| "?".to_owned(), format_size))
        .collect();
    let size_width = column_width("SIZE", size_strs.iter().map(String::as_str));

    if show_status_and_pulls {
        let status_width = column_width("STATUS", rows.iter().map(|row| row.status.as_str()));
        let access_width = column_width("ACCESS", rows.iter().map(|row| row.access.as_str()));
        let pulls_strs: Vec<String> = rows
            .iter()
            .map(|row| row.pulls.map_or_else(|| "?".to_owned(), format_count))
            .collect();
        let pulls_width = column_width("PULLS", pulls_strs.iter().map(String::as_str));

        println!(
            "{:<sw$}  {:<aw$}  {:>zw$}  {:>pw$}  MODEL",
            "STATUS",
            "ACCESS",
            "SIZE",
            "PULLS",
            sw = status_width,
            aw = access_width,
            zw = size_width,
            pw = pulls_width
        );
        for ((row, size_str), pulls_str) in rows.iter().zip(&size_strs).zip(&pulls_strs) {
            println!(
                "{:<sw$}  {:<aw$}  {:>zw$}  {:>pw$}  {}",
                row.status,
                row.access,
                size_str,
                pulls_str,
                row.model,
                sw = status_width,
                aw = access_width,
                zw = size_width,
                pw = pulls_width
            );
        }
    } else {
        println!("{:>zw$}  MODEL", "SIZE", zw = size_width);
        for (row, size_str) in rows.iter().zip(&size_strs) {
            println!("{size_str:>size_width$}  {}", row.model);
        }
    }
}

fn column_width<'a>(header: &str, values: impl Iterator<Item = &'a str>) -> usize {
    values.fold(header.len(), |max, value| max.max(value.len()))
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{size:.2} {}", UNITS[unit])
    }
}

fn format_count(n: u64) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{}k", ((n as f64) / 1_000.0).round() as u64)
    } else {
        format!("{:.1}M", (n as f64) / 1_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_stays_in_bytes_below_a_kibibyte() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn format_size_crosses_unit_boundaries() {
        assert_eq!(format_size(1024), "1.00 KiB");
        assert_eq!(format_size(1024 * 1024), "1.00 MiB");
        assert_eq!(format_size(5214561328), "4.86 GiB");
    }

    #[test]
    fn format_count_below_a_thousand_is_exact() {
        assert_eq!(format_count(999), "999");
    }

    #[test]
    fn format_count_thousands_round_to_nearest() {
        assert_eq!(format_count(339377), "339k");
        assert_eq!(format_count(852802), "853k");
    }

    #[test]
    fn format_count_millions_keep_one_decimal() {
        assert_eq!(format_count(3000602), "3.0M");
    }

    #[test]
    fn column_width_grows_to_fit_a_long_ref() {
        let long = "stabilityai/sdxl-turbo:sd_xl_turbo_1.0_fp16.safetensors";
        assert_eq!(column_width("MODEL", ["short", long].into_iter()), long.len());
    }

    #[test]
    fn column_width_defaults_to_header_length() {
        assert_eq!(column_width("STATUS", std::iter::empty()), "STATUS".len());
    }
}
