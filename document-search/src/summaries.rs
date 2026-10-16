//! `summaries` subcommand: read one language tree of a document's
//! `document_summary` rows and print its shape or any slice of it.
//!
//! Runs inline in the connection task (it bypasses the worker queue), so it
//! reads through `Db::fresh_conn()` to stay out of an in-flight ingest's
//! transaction. A tree is at most a few hundred rows, so it's loaded whole and
//! filtered in memory.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

use turso::Value;

use crate::commands::{self, CommandError, DocumentRow};
use crate::db::Db;
use crate::protocol::SummarySelect;

/// Config needed to pick the default language tree. Copied into
/// `ServerState` because inline handlers don't hold the full `Config`.
#[derive(Debug, Clone)]
pub struct LanguagePrefs {
    pub search_language: Option<String>,
    pub summarize_languages: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Query {
    pub language: Option<String>,
    pub level: Option<u32>,
    pub select: SummarySelect,
    pub outline: bool,
}

#[derive(Debug, Clone)]
struct Row {
    id: i64,
    parent_id: Option<i64>,
    level: u32,
    byte_start: i64,
    /// Exclusive, like every other byte_end in the DB.
    byte_end: i64,
    page_first: Option<i64>,
    page_last: Option<i64>,
    text: String,
}

#[derive(Debug)]
struct Node {
    row: Row,
    /// 0-based position within its level, in byte order.
    index: u32,
    parent_index: Option<u32>,
    /// Inclusive index range of this node's children one level down.
    children: Option<(u32, u32)>,
}

/// `levels[k]` holds level k's nodes in byte order. A level can be empty only
/// in a damaged tree with a gap in its levels.
#[derive(Debug)]
struct Tree {
    levels: Vec<Vec<Node>>,
}

/// Everything a renderer needs.
struct View<'a> {
    path: &'a Path,
    language: String,
    available: Vec<String>,
    tree: Tree,
    /// `(level, index)` of each selected node, top level first.
    selected: Vec<(u32, u32)>,
    outline: bool,
}

pub async fn summaries_text(
    db: &Db,
    raw_path: &Path,
    query: &Query,
    prefs: &LanguagePrefs,
) -> Result<String, CommandError> {
    let path = commands::normalize_path(raw_path);
    let view = load(db, &path, query, prefs).await?;
    Ok(render_text(&view))
}

pub async fn summaries_json(
    db: &Db,
    raw_path: &Path,
    query: &Query,
    prefs: &LanguagePrefs,
) -> Result<String, CommandError> {
    let path = commands::normalize_path(raw_path);
    let view = load(db, &path, query, prefs).await?;
    Ok(render_json(&view).to_string())
}

async fn load<'a>(
    db: &Db,
    path: &'a Path,
    query: &Query,
    prefs: &LanguagePrefs,
) -> Result<View<'a>, CommandError> {
    let conn = db
        .fresh_conn()
        .await
        .map_err(|source| CommandError::FreshConn { source })?;
    let doc = commands::lookup_document(&conn, path).await?;
    let available: Vec<String> = commands::fetch_summary_counts(&conn, doc.id)
        .await?
        .into_iter()
        .map(|(lang, _)| lang)
        .collect();
    let language = choose_language(query.language.as_deref(), &available, prefs, &doc, path)?;
    let rows = fetch_rows(&conn, doc.id, &language).await?;
    let tree = build_tree(rows);
    let selected = select(&tree, query.level, &query.select, &doc, path)?;
    Ok(View {
        path,
        language,
        available,
        tree,
        selected,
        outline: query.outline,
    })
}

fn choose_language(
    requested: Option<&str>,
    available: &[String],
    prefs: &LanguagePrefs,
    doc: &DocumentRow,
    path: &Path,
) -> Result<String, CommandError> {
    if available.is_empty() {
        return Err(CommandError::NoSummaries {
            path: path.to_path_buf(),
        });
    }
    let language = match requested {
        Some(l) => crate::language::normalize(l),
        None if available.len() == 1 => return Ok(available[0].clone()),
        None => crate::language::preferred_summary_language(
            prefs.search_language.as_deref(),
            &prefs.summarize_languages,
            doc.language.as_deref(),
        ),
    };
    if available.contains(&language) {
        Ok(language)
    } else {
        Err(CommandError::SummaryLanguageMissing {
            language,
            available: available.to_vec(),
        })
    }
}

async fn fetch_rows(
    conn: &turso::Connection,
    document_id: i64,
    language: &str,
) -> Result<Vec<Row>, CommandError> {
    let db_err = |op: &'static str| move |source| CommandError::Db { op, source };
    let mut rows = conn
        .query(
            "SELECT id, parent_id, level, byte_start, byte_end, page_first, page_last, text \
             FROM document_summary WHERE document_id = ? AND language = ? \
             ORDER BY level, byte_start",
            (document_id, language.to_string()),
        )
        .await
        .map_err(db_err("summaries.fetch"))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err("summaries.fetch.next"))? {
        let opt_int = |i: usize| -> Result<Option<i64>, CommandError> {
            match row.get_value(i).map_err(db_err("summaries.fetch.get"))? {
                Value::Integer(n) => Ok(Some(n)),
                _ => Ok(None),
            }
        };
        let level: i64 = row.get(2).map_err(db_err("summaries.fetch.get.level"))?;
        out.push(Row {
            id: row.get(0).map_err(db_err("summaries.fetch.get.id"))?,
            parent_id: opt_int(1)?,
            level: level as u32,
            byte_start: row.get(3).map_err(db_err("summaries.fetch.get.byte_start"))?,
            byte_end: row.get(4).map_err(db_err("summaries.fetch.get.byte_end"))?,
            page_first: opt_int(5)?,
            page_last: opt_int(6)?,
            text: row.get(7).map_err(db_err("summaries.fetch.get.text"))?,
        });
    }
    Ok(out)
}

/// Index each level in byte order and link parents/children by position. A
/// partial tree (summarize still running, or interrupted) just leaves some
/// `parent_index` / `children` empty.
fn build_tree(mut rows: Vec<Row>) -> Tree {
    rows.sort_by_key(|r| (r.level, r.byte_start, r.id));
    let depth = rows.iter().map(|r| r.level + 1).max().unwrap_or(0) as usize;
    let mut levels: Vec<Vec<Node>> = (0..depth).map(|_| Vec::new()).collect();
    for row in rows {
        let level = &mut levels[row.level as usize];
        let index = level.len() as u32;
        level.push(Node {
            row,
            index,
            parent_index: None,
            children: None,
        });
    }

    let position: HashMap<i64, (u32, u32)> = levels
        .iter()
        .flatten()
        .map(|n| (n.row.id, (n.row.level, n.index)))
        .collect();
    let mut links: Vec<(u32, u32, u32)> = Vec::new(); // (child level, child index, parent index)
    for node in levels.iter().flatten() {
        if let Some(&(plevel, pindex)) = node.row.parent_id.and_then(|p| position.get(&p))
            && plevel == node.row.level + 1
        {
            links.push((node.row.level, node.index, pindex));
        }
    }
    for (clevel, cindex, pindex) in links {
        levels[clevel as usize][cindex as usize].parent_index = Some(pindex);
        let parent = &mut levels[clevel as usize + 1][pindex as usize];
        parent.children = Some(match parent.children {
            None => (cindex, cindex),
            Some((a, b)) => (a.min(cindex), b.max(cindex)),
        });
    }
    Tree { levels }
}

fn select(
    tree: &Tree,
    level: Option<u32>,
    sel: &SummarySelect,
    doc: &DocumentRow,
    path: &Path,
) -> Result<Vec<(u32, u32)>, CommandError> {
    let depth = tree.levels.len() as u32;
    if let Some(l) = level
        && tree.levels.get(l as usize).is_none_or(|v| v.is_empty())
    {
        return Err(CommandError::SummaryLevelMissing { level: l, count: depth });
    }

    let keep: Box<dyn Fn(&Node) -> bool> = match *sel {
        SummarySelect::All => Box::new(|_| true),
        SummarySelect::Index { first, last } => {
            let l = level.expect("clap requires --level with --index");
            if first > last {
                return Err(CommandError::InvalidRange {
                    start: first as u64,
                    end: last as u64,
                });
            }
            let count = tree.levels[l as usize].len() as u32;
            if last >= count {
                return Err(CommandError::SummaryIndexOutOfBounds {
                    level: l,
                    first,
                    last,
                    count,
                });
            }
            Box::new(move |n| (first..=last).contains(&n.index))
        }
        SummarySelect::Pages { first, last } => {
            if first > last {
                return Err(CommandError::InvalidPageRange { first, last });
            }
            let total = doc.total_size_pages.ok_or_else(|| CommandError::NotPdf {
                path: path.to_path_buf(),
            })? as u32;
            if first == 0 || last > total {
                return Err(CommandError::PageRangeOutOfBounds { first, last, total });
            }
            let (first, last) = (first as i64, last as i64);
            Box::new(move |n| match (n.row.page_first, n.row.page_last) {
                (Some(pf), Some(pl)) => pf <= last && pl >= first,
                _ => false,
            })
        }
        SummarySelect::Bytes { start, end } => {
            if start > end {
                return Err(CommandError::InvalidRange { start, end });
            }
            let total = doc.total_size_bytes as u64;
            if end >= total {
                return Err(CommandError::ByteRangeOutOfBounds { start, end, total });
            }
            let (start, end) = (start as i64, end as i64);
            Box::new(move |n| n.row.byte_start <= end && n.row.byte_end > start)
        }
    };

    Ok(tree
        .levels
        .iter()
        .rev()
        .flatten()
        .filter(|n| level.is_none_or(|l| n.row.level == l))
        .filter(|n| keep(n))
        .map(|n| (n.row.level, n.index))
        .collect())
}

impl Tree {
    fn node(&self, level: u32, index: u32) -> &Node {
        &self.levels[level as usize][index as usize]
    }
}

fn pages_label(n: &Node) -> Option<String> {
    match (n.row.page_first, n.row.page_last) {
        (Some(a), Some(b)) if a == b => Some(format!("page {a}")),
        (Some(a), Some(b)) => Some(format!("pages {a}-{b}")),
        _ => None,
    }
}

/// Inclusive, so it can be pasted into `text --bytes`.
fn bytes_label(n: &Node) -> String {
    format!("bytes {}-{}", n.row.byte_start, n.row.byte_end - 1)
}

fn render_text(view: &View) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "path:      {}", view.path.display());
    let _ = writeln!(out, "language:  {}", view.language);
    if view.available.len() > 1 {
        let _ = writeln!(out, "available: {}", view.available.join(", "));
    }
    let _ = writeln!(out, "levels:    {}", view.tree.levels.len());
    for (k, level) in view.tree.levels.iter().enumerate().rev() {
        let noun = if level.len() == 1 { "summary" } else { "summaries" };
        let _ = writeln!(out, "  level {k}: {} {noun}", level.len());
    }

    for &(level, index) in &view.selected {
        let n = view.tree.node(level, index);
        let count = view.tree.levels[level as usize].len();
        let mut loc = format!("level {level} #{index} of {count}");
        if let Some(p) = pages_label(n) {
            let _ = write!(loc, " · {p}");
        }
        let _ = write!(loc, " · {}", bytes_label(n));
        if view.outline {
            let _ = write!(out, "\n{loc} · id {}", n.row.id);
            if let Some(p) = n.parent_index {
                let _ = write!(out, " · parent #{p}");
            }
            if let Some((a, b)) = n.children {
                let _ = write!(out, " · children #{a}-#{b}");
            }
        } else {
            let _ = write!(out, "\n== {loc} ==\n{}\n", n.row.text.trim_end());
        }
    }
    out.push('\n');
    out
}

fn render_json(view: &View) -> serde_json::Value {
    let levels: Vec<serde_json::Value> = view
        .tree
        .levels
        .iter()
        .enumerate()
        .map(|(k, v)| serde_json::json!({"level": k, "count": v.len()}))
        .collect();
    let summaries: Vec<serde_json::Value> = view
        .selected
        .iter()
        .map(|&(level, index)| {
            let n = view.tree.node(level, index);
            let mut obj = serde_json::json!({
                "id": n.row.id,
                "level": level,
                "index": index,
                "parent_index": n.parent_index,
                "children": n.children.map(|(a, b)| serde_json::json!({"first": a, "last": b})),
                "page_first": n.row.page_first,
                "page_last": n.row.page_last,
                "byte_start": n.row.byte_start,
                "byte_end": n.row.byte_end,
            });
            if !view.outline {
                obj["text"] = serde_json::Value::from(n.row.text.as_str());
            }
            obj
        })
        .collect();
    serde_json::json!({
        "ok": true,
        "path": view.path.display().to_string(),
        "language": view.language,
        "available_languages": view.available,
        "levels": levels,
        "summaries": summaries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, parent_id: Option<i64>, level: u32, bytes: (i64, i64), pages: (i64, i64)) -> Row {
        Row {
            id,
            parent_id,
            level,
            byte_start: bytes.0,
            byte_end: bytes.1,
            page_first: Some(pages.0),
            page_last: Some(pages.1),
            text: format!("summary {id}"),
        }
    }

    /// Root 10 over L1 {20, 21}; 20 over L0 {30, 31}, 21 over L0 {32}.
    /// Rows are deliberately out of order.
    fn sample_rows() -> Vec<Row> {
        vec![
            row(32, Some(21), 0, (200, 300), (7, 9)),
            row(10, None, 2, (0, 300), (1, 9)),
            row(21, Some(10), 1, (200, 300), (7, 9)),
            row(30, Some(20), 0, (0, 100), (1, 3)),
            row(20, Some(10), 1, (0, 200), (1, 6)),
            row(31, Some(20), 0, (100, 200), (3, 6)),
        ]
    }

    fn doc(pages: Option<i64>) -> DocumentRow {
        DocumentRow {
            id: 1,
            doc_type: if pages.is_some() { "pdf" } else { "txt" }.into(),
            total_size_bytes: 300,
            total_size_chars: 300,
            total_size_pages: pages,
            language: Some("ja".into()),
        }
    }

    fn ids(tree: &Tree, sel: &[(u32, u32)]) -> Vec<i64> {
        sel.iter().map(|&(l, i)| tree.node(l, i).row.id).collect()
    }

    fn run(tree: &Tree, level: Option<u32>, sel: SummarySelect) -> Result<Vec<i64>, CommandError> {
        select(tree, level, &sel, &doc(Some(9)), Path::new("/d.pdf")).map(|s| ids(tree, &s))
    }

    #[test]
    fn build_tree_indexes_and_links() {
        let tree = build_tree(sample_rows());
        let counts: Vec<usize> = tree.levels.iter().map(Vec::len).collect();
        assert_eq!(counts, vec![3, 2, 1]);
        assert_eq!(tree.node(0, 2).row.id, 32);
        assert_eq!(tree.node(0, 2).parent_index, Some(1));
        assert_eq!(tree.node(1, 0).children, Some((0, 1)));
        assert_eq!(tree.node(1, 1).children, Some((2, 2)));
        assert_eq!(tree.node(2, 0).children, Some((0, 1)));
        assert_eq!(tree.node(2, 0).parent_index, None);
    }

    #[test]
    fn build_tree_tolerates_partial_tree() {
        // Level 1 still being built: no root, one L0 node not yet parented.
        let rows = vec![
            row(30, Some(20), 0, (0, 100), (1, 3)),
            row(31, None, 0, (100, 200), (3, 6)),
            row(20, None, 1, (0, 100), (1, 3)),
        ];
        let tree = build_tree(rows);
        assert_eq!(tree.levels.len(), 2);
        assert_eq!(tree.node(0, 1).parent_index, None);
        assert_eq!(tree.node(1, 0).children, Some((0, 0)));
    }

    #[test]
    fn select_all_is_top_down() {
        let tree = build_tree(sample_rows());
        assert_eq!(run(&tree, None, SummarySelect::All).unwrap(), vec![10, 20, 21, 30, 31, 32]);
        assert_eq!(run(&tree, Some(1), SummarySelect::All).unwrap(), vec![20, 21]);
    }

    #[test]
    fn select_index_range_and_errors() {
        let tree = build_tree(sample_rows());
        let sel = SummarySelect::Index { first: 1, last: 2 };
        assert_eq!(run(&tree, Some(0), sel).unwrap(), vec![31, 32]);
        assert!(matches!(
            run(&tree, Some(0), SummarySelect::Index { first: 0, last: 3 }),
            Err(CommandError::SummaryIndexOutOfBounds { count: 3, .. })
        ));
        assert!(matches!(
            run(&tree, Some(0), SummarySelect::Index { first: 2, last: 1 }),
            Err(CommandError::InvalidRange { .. })
        ));
        assert!(matches!(
            run(&tree, Some(5), SummarySelect::All),
            Err(CommandError::SummaryLevelMissing { level: 5, count: 3 })
        ));
    }

    #[test]
    fn select_pages_overlap_and_errors() {
        let tree = build_tree(sample_rows());
        let sel = SummarySelect::Pages { first: 6, last: 7 };
        assert_eq!(run(&tree, None, sel.clone()).unwrap(), vec![10, 20, 21, 31, 32]);
        assert_eq!(run(&tree, Some(0), sel.clone()).unwrap(), vec![31, 32]);
        assert!(matches!(
            run(&tree, None, SummarySelect::Pages { first: 0, last: 2 }),
            Err(CommandError::PageRangeOutOfBounds { .. })
        ));
        assert!(matches!(
            run(&tree, None, SummarySelect::Pages { first: 1, last: 10 }),
            Err(CommandError::PageRangeOutOfBounds { total: 9, .. })
        ));
        let txt = select(&tree, None, &sel, &doc(None), Path::new("/d.txt"));
        assert!(matches!(txt, Err(CommandError::NotPdf { .. })));
    }

    #[test]
    fn select_bytes_respects_exclusive_end() {
        let tree = build_tree(sample_rows());
        // Byte 100 is the first byte of 31, not part of 30 (30 ends at 100 exclusive).
        let sel = SummarySelect::Bytes { start: 100, end: 100 };
        assert_eq!(run(&tree, Some(0), sel).unwrap(), vec![31]);
        assert!(matches!(
            run(&tree, None, SummarySelect::Bytes { start: 0, end: 300 }),
            Err(CommandError::ByteRangeOutOfBounds { .. })
        ));
    }

    #[test]
    fn choose_language_rules() {
        let prefs = LanguagePrefs {
            search_language: None,
            summarize_languages: vec!["source".into(), "en".into()],
        };
        let d = doc(Some(9));
        let p = Path::new("/d.pdf");
        let one = vec!["en".to_string()];
        let two = vec!["en".to_string(), "ja".to_string()];
        assert_eq!(choose_language(None, &one, &prefs, &d, p).unwrap(), "en");
        // "source" resolves to the document's language (ja).
        assert_eq!(choose_language(None, &two, &prefs, &d, p).unwrap(), "ja");
        assert_eq!(choose_language(Some(" EN "), &two, &prefs, &d, p).unwrap(), "en");
        assert!(matches!(
            choose_language(Some("fr"), &two, &prefs, &d, p),
            Err(CommandError::SummaryLanguageMissing { .. })
        ));
        assert!(matches!(
            choose_language(None, &[], &prefs, &d, p),
            Err(CommandError::NoSummaries { .. })
        ));
    }
}
