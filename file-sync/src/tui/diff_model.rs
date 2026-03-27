use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_no: Option<usize>,
    pub text: String,
    pub tag: ChangeTag,
}

#[derive(Debug, Clone)]
pub struct DiffRow {
    pub columns: Vec<Option<DiffLine>>,
}

pub fn build_side_by_side(left_text: &str, right_text: &str) -> Vec<DiffRow> {
    let diff = TextDiff::from_lines(left_text, right_text);
    let mut rows = Vec::new();
    let mut left_line = 1usize;
    let mut right_line = 1usize;

    for op in diff.ops() {
            match op.tag() {
                similar::DiffTag::Equal => {
                    for change in diff.iter_changes(op) {
                        rows.push(DiffRow {
                            columns: vec![
                                Some(DiffLine {
                                    line_no: Some(left_line),
                                    text: change.to_string_lossy().to_string(),
                                    tag: ChangeTag::Equal,
                                }),
                                Some(DiffLine {
                                    line_no: Some(right_line),
                                    text: change.to_string_lossy().to_string(),
                                    tag: ChangeTag::Equal,
                                }),
                            ],
                        });
                        left_line += 1;
                        right_line += 1;
                    }
                }
                similar::DiffTag::Replace => {
                    let old_changes: Vec<_> = diff
                        .iter_changes(op)
                        .filter(|c| c.tag() == ChangeTag::Delete)
                        .collect();
                    let new_changes: Vec<_> = diff
                        .iter_changes(op)
                        .filter(|c| c.tag() == ChangeTag::Insert)
                        .collect();

                    let max_len = old_changes.len().max(new_changes.len());
                    for i in 0..max_len {
                        let left = if i < old_changes.len() {
                            let l = left_line;
                            left_line += 1;
                            Some(DiffLine {
                                line_no: Some(l),
                                text: old_changes[i].to_string_lossy().to_string(),
                                tag: ChangeTag::Delete,
                            })
                        } else {
                            None
                        };
                        let right = if i < new_changes.len() {
                            let l = right_line;
                            right_line += 1;
                            Some(DiffLine {
                                line_no: Some(l),
                                text: new_changes[i].to_string_lossy().to_string(),
                                tag: ChangeTag::Insert,
                            })
                        } else {
                            None
                        };
                        rows.push(DiffRow { columns: vec![left, right] });
                    }
                }
                similar::DiffTag::Delete => {
                    for change in diff.iter_changes(op) {
                        rows.push(DiffRow {
                            columns: vec![
                                Some(DiffLine {
                                    line_no: Some(left_line),
                                    text: change.to_string_lossy().to_string(),
                                    tag: ChangeTag::Delete,
                                }),
                                None,
                            ],
                        });
                        left_line += 1;
                    }
                }
                similar::DiffTag::Insert => {
                    for change in diff.iter_changes(op) {
                        rows.push(DiffRow {
                            columns: vec![
                                None,
                                Some(DiffLine {
                                    line_no: Some(right_line),
                                    text: change.to_string_lossy().to_string(),
                                    tag: ChangeTag::Insert,
                                }),
                            ],
                        });
                        right_line += 1;
                    }
                }
            }
    }

    rows
}

pub fn build_multi_column(texts: &[&str]) -> Vec<DiffRow> {
    let line_vecs: Vec<Vec<&str>> = texts.iter().map(|t| t.lines().collect()).collect();
    let max_lines = line_vecs.iter().map(|v| v.len()).max().unwrap_or(0);

    let mut rows = Vec::with_capacity(max_lines);
    for line_idx in 0..max_lines {
        let ref_text = line_vecs.first().and_then(|lines| lines.get(line_idx).copied());
        let all_same = line_vecs
            .iter()
            .all(|lines| lines.get(line_idx).copied() == ref_text);

        let columns: Vec<Option<DiffLine>> = line_vecs
            .iter()
            .map(|lines| {
                if line_idx < lines.len() {
                    let tag = if all_same {
                        ChangeTag::Equal
                    } else {
                        ChangeTag::Insert
                    };
                    Some(DiffLine {
                        line_no: Some(line_idx + 1),
                        text: format!("{}\n", lines[line_idx]),
                        tag,
                    })
                } else {
                    None
                }
            })
            .collect();
        rows.push(DiffRow { columns });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_text() {
        let text = "line one\nline two\n";
        let rows = build_side_by_side(text, text);
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert!(row.columns[0].is_some());
            assert!(row.columns[1].is_some());
            assert_eq!(row.columns[0].as_ref().unwrap().tag, ChangeTag::Equal);
        }
    }

    #[test]
    fn test_insertion() {
        let left = "line one\nline three\n";
        let right = "line one\nline two\nline three\n";
        let rows = build_side_by_side(left, right);
        let insert_rows: Vec<_> = rows
            .iter()
            .filter(|r| {
                r.columns[1]
                    .as_ref()
                    .map(|d| d.tag == ChangeTag::Insert)
                    .unwrap_or(false)
            })
            .collect();
        assert!(!insert_rows.is_empty());
    }

    #[test]
    fn test_deletion() {
        let left = "line one\nline two\nline three\n";
        let right = "line one\nline three\n";
        let rows = build_side_by_side(left, right);
        let delete_rows: Vec<_> = rows
            .iter()
            .filter(|r| {
                r.columns[0]
                    .as_ref()
                    .map(|d| d.tag == ChangeTag::Delete)
                    .unwrap_or(false)
            })
            .collect();
        assert!(!delete_rows.is_empty());
    }

    #[test]
    fn test_replacement() {
        let left = "hello world\n";
        let right = "goodbye world\n";
        let rows = build_side_by_side(left, right);
        assert!(!rows.is_empty());
        let has_replace = rows.iter().any(|r| {
            r.columns[0]
                .as_ref()
                .map(|d| d.tag == ChangeTag::Delete)
                .unwrap_or(false)
                && r.columns[1]
                    .as_ref()
                    .map(|d| d.tag == ChangeTag::Insert)
                    .unwrap_or(false)
        });
        assert!(has_replace);
    }

    #[test]
    fn test_line_numbers() {
        let left = "a\nb\nc\n";
        let right = "a\nx\nc\n";
        let rows = build_side_by_side(left, right);
        let left_nums: Vec<usize> = rows
            .iter()
            .filter_map(|r| r.columns[0].as_ref().and_then(|d| d.line_no))
            .collect();
        for w in left_nums.windows(2) {
            assert!(w[1] >= w[0]);
        }
    }

    #[test]
    fn test_multi_column_basic() {
        let rows = build_multi_column(&["a\nb\n", "a\nc\n", "x\ny\nz\n"]);
        assert_eq!(rows.len(), 3); // max lines across all texts
        assert_eq!(rows[0].columns.len(), 3);
        // First two texts have 2 lines, third has 3
        assert!(rows[2].columns[0].is_none());
        assert!(rows[2].columns[1].is_none());
        assert!(rows[2].columns[2].is_some());
        // Row 0: "a" vs "a" vs "x" — not all same, should be highlighted
        assert_eq!(rows[0].columns[0].as_ref().unwrap().tag, ChangeTag::Insert);
        // Row 1: "b" vs "c" vs "y" — not all same
        assert_eq!(rows[1].columns[0].as_ref().unwrap().tag, ChangeTag::Insert);
    }

    #[test]
    fn test_multi_column_equal_lines() {
        let rows = build_multi_column(&["same\ndiff\n", "same\nother\n"]);
        // Row 0: "same" vs "same" — equal
        assert_eq!(rows[0].columns[0].as_ref().unwrap().tag, ChangeTag::Equal);
        // Row 1: "diff" vs "other" — different
        assert_eq!(rows[1].columns[0].as_ref().unwrap().tag, ChangeTag::Insert);
    }
}
