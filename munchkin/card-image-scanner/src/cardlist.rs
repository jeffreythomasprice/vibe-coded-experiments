//! Parse the known card-name list and fuzzy-correct OCR'd titles against it.

use anyhow::{Context, Result};
use regex::Regex;
use std::path::Path;
use strsim::jaro_winkler;

pub struct CardList {
    /// Canonical card names as they appear in the list.
    names: Vec<String>,
    /// Pre-normalized form, parallel to `names`.
    normalized: Vec<String>,
}

impl CardList {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading card list {path:?}"))?;
        Ok(Self::parse(&text))
    }

    pub fn parse(text: &str) -> Self {
        let count_re = Regex::new(r"\s*\(\d+\)\s*$").unwrap();
        let mut names = Vec::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            // Skip header / metadata lines.
            if line == "Munchkin"
                || line.starts_with("Door Cards:")
                || line.starts_with("Treasure Cards:")
            {
                continue;
            }
            // Drop a trailing "(N)" copy-count.
            let name = count_re.replace(line, "").to_string();
            if !name.is_empty() {
                names.push(name);
            }
        }
        let normalized = names.iter().map(|n| normalize(n)).collect();
        CardList { names, normalized }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Best canonical match for an OCR'd title, with its similarity score.
    ///
    /// Besides comparing the whole normalized title, this slides a window the
    /// width of each candidate (in words) across the title and keeps the best
    /// score. That recovers names embedded behind prefix labels, e.g.
    /// "LEVEL 8 FACE SUCKER" → "Face Sucker" or
    /// "Usable by Wizard Only STAFF OF NAPALM" → "Staff of Napalm".
    /// Returns `None` if the input is empty.
    pub fn best_match(&self, raw_title: &str) -> Option<(String, f64)> {
        let norm = normalize(raw_title);
        if norm.is_empty() {
            return None;
        }
        let title_tokens: Vec<&str> = norm.split(' ').collect();

        let mut best: Option<(usize, f64)> = None;
        for (i, cand) in self.normalized.iter().enumerate() {
            let mut score = jaro_winkler(&norm, cand);

            // Windowed match: align the candidate against same-width slices.
            let k = cand.split(' ').count();
            if k >= 1 && title_tokens.len() > k {
                for w in title_tokens.windows(k) {
                    let window = w.join(" ");
                    let s = jaro_winkler(&window, cand);
                    if s > score {
                        score = s;
                    }
                }
            }

            if best.map_or(true, |(_, s)| score > s) {
                best = Some((i, score));
            }
        }
        best.map(|(i, s)| (self.names[i].clone(), s))
    }
}

/// Normalize for comparison: uppercase, strip punctuation, collapse whitespace.
fn normalize(s: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            for u in ch.to_uppercase() {
                out.push(u);
            }
            prev_space = false;
        } else if ch.is_whitespace() || ch == '-' {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        }
        // other punctuation dropped
    }
    out.trim().to_string()
}
