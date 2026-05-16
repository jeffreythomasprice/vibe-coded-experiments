//! DB hygiene: detect and repair orphan rows left by interrupted summary
//! builds from older versions (before the summarizer learned to roll back
//! its inserts on cancel).
//!
//! Trigger: a summary build (now run as the final phase of ingest) is
//! interrupted mid-`update_summary_parent` loop — some level-(k-1) summaries
//! get their `parent_id` set, others stay NULL. On the next run the
//! per-group content-hash skip can't repair this because the level-k rows
//! already exist but their children aren't all pointing at them. Cleanest
//! fix is to drop the entire topmost level for the affected document; the
//! user re-ingests to rebuild it.

use turso::{Value, params::Params};

use crate::db::Db;

#[derive(thiserror::Error, Debug)]
pub enum CleanupError {
    #[error("db error during cleanup ({op}): {source}")]
    Db {
        op: &'static str,
        #[source]
        source: turso::Error,
    },
}

#[derive(Debug, Default, Clone)]
pub struct CleanupReport {
    /// Number of documents that had at least one partial level repaired.
    pub documents_repaired: usize,
    /// Total `document_summary` rows deleted across all repairs.
    pub summary_rows_removed: usize,
}

impl CleanupReport {
    pub fn is_empty(&self) -> bool {
        self.documents_repaired == 0 && self.summary_rows_removed == 0
    }
}

/// Scan for documents whose summary tree has a "partial" level (mixed
/// NULL/non-NULL parent_id at the same level — the signature of an
/// interrupted `update_summary_parent` loop) and repair them in place.
///
/// Repair strategy: for each affected document, identify the *bottommost*
/// level whose rows have mixed parent linkage. Everything at higher levels
/// was built on top of a partial foundation, so drop levels >= that level
/// for the document. (`document_summary.parent_id` references the table
/// itself with `ON DELETE CASCADE`, so we must NULL out child references
/// before deleting to avoid wiping pre-existing rows we want to keep.)
pub async fn scan_and_repair(db: &Db) -> Result<CleanupReport, CleanupError> {
    let affected = find_partial_documents(db).await?;
    let mut report = CleanupReport::default();
    for (document_id, min_partial_level) in affected {
        let removed = repair_document(db, document_id, min_partial_level).await?;
        if removed > 0 {
            report.documents_repaired += 1;
            report.summary_rows_removed += removed;
        }
    }
    Ok(report)
}

/// Returns `(document_id, min_level_with_partial_linkage)` for each affected
/// document. The "min" is the lowest level whose children are inconsistently
/// linked to a parent — that's the level that was being built when the run
/// was interrupted. Repair drops level + 1 (the parents that were being
/// wired up) and everything above; level itself is kept intact.
async fn find_partial_documents(db: &Db) -> Result<Vec<(i64, i64)>, CleanupError> {
    let mut rows = db
        .conn
        .query(
            "SELECT document_id, level, \
                    SUM(parent_id IS NULL) AS nulls, \
                    SUM(parent_id IS NOT NULL) AS set_count \
             FROM document_summary \
             GROUP BY document_id, level \
             HAVING nulls > 0 AND set_count > 0 \
             ORDER BY document_id, level",
            (),
        )
        .await
        .map_err(|source| CleanupError::Db {
            op: "find_partial.query",
            source,
        })?;

    let mut out: Vec<(i64, i64)> = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| CleanupError::Db {
        op: "find_partial.next",
        source,
    })? {
        let doc_id: i64 = row.get(0).map_err(|source| CleanupError::Db {
            op: "find_partial.get.doc_id",
            source,
        })?;
        let level: i64 = row.get(1).map_err(|source| CleanupError::Db {
            op: "find_partial.get.level",
            source,
        })?;
        // Keep only the smallest partial level per document; the result is
        // ordered by (document_id, level) so the first row wins.
        if out.last().map(|(d, _)| *d) != Some(doc_id) {
            out.push((doc_id, level));
        }
    }
    Ok(out)
}

/// Delete summary rows at level `min_partial_level + 1` and above for this
/// document.
///
/// Turso 0.4's self-referential `ON DELETE CASCADE` on
/// `document_summary.parent_id` overflows the runtime stack, so we do the
/// deletion on a dedicated connection with foreign keys turned off and
/// clean up dependent rows manually.
async fn repair_document(
    db: &Db,
    document_id: i64,
    min_partial_level: i64,
) -> Result<usize, CleanupError> {
    let drop_from = min_partial_level + 1;

    // First, collect the ids we'll be deleting (count for the report and so
    // we can target dependent rows by id, not by complex WHERE clauses).
    let mut ids: Vec<i64> = Vec::new();
    let mut rows = db
        .conn
        .query(
            "SELECT id FROM document_summary \
             WHERE document_id = ? AND level >= ? \
             ORDER BY level DESC",
            Params::Positional(vec![
                Value::Integer(document_id),
                Value::Integer(drop_from),
            ]),
        )
        .await
        .map_err(|source| CleanupError::Db {
            op: "repair.collect_ids.query",
            source,
        })?;
    while let Some(row) = rows.next().await.map_err(|source| CleanupError::Db {
        op: "repair.collect_ids.next",
        source,
    })? {
        let id: i64 = row.get(0).map_err(|source| CleanupError::Db {
            op: "repair.collect_ids.get",
            source,
        })?;
        ids.push(id);
    }
    if ids.is_empty() {
        return Ok(0);
    }

    let conn = db.database.connect().map_err(|source| CleanupError::Db {
        op: "repair.connect",
        source,
    })?;
    conn.execute("PRAGMA foreign_keys = OFF", ())
        .await
        .map_err(|source| CleanupError::Db {
            op: "repair.pragma",
            source,
        })?;

    let vec_table = db.summary_vectors_table.as_str();
    let del_vec_sql = format!("DELETE FROM {vec_table} WHERE summary_id = ?");
    for &id in &ids {
        conn.execute(&del_vec_sql, Params::Positional(vec![Value::Integer(id)]))
            .await
            .map_err(|source| CleanupError::Db {
                op: "repair.delete_vector",
                source,
            })?;
        conn.execute(
            "UPDATE document_summary SET parent_id = NULL WHERE parent_id = ?",
            Params::Positional(vec![Value::Integer(id)]),
        )
        .await
        .map_err(|source| CleanupError::Db {
            op: "repair.null_children",
            source,
        })?;
        conn.execute(
            "DELETE FROM document_summary WHERE id = ?",
            Params::Positional(vec![Value::Integer(id)]),
        )
        .await
        .map_err(|source| CleanupError::Db {
            op: "repair.delete_row",
            source,
        })?;
    }

    Ok(ids.len())
}

pub fn render_text(report: &CleanupReport) -> String {
    if report.is_empty() {
        "cleanup: nothing to repair\n".to_string()
    } else {
        format!(
            "cleanup: repaired {} document(s), removed {} summary row(s)\n",
            report.documents_repaired, report.summary_rows_removed,
        )
    }
}

pub fn render_json(report: &CleanupReport) -> String {
    serde_json::json!({
        "ok": true,
        "documents_repaired": report.documents_repaired,
        "summary_rows_removed": report.summary_rows_removed,
    })
    .to_string()
}
