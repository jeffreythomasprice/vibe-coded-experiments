use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_no: Option<usize>,
    pub text: String,
    pub tag: ChangeTag,
}

#[derive(Debug, Clone)]
pub struct DiffRow {
    pub left: Option<DiffLine>,
    pub right: Option<DiffLine>,
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
                            left: Some(DiffLine {
                                line_no: Some(left_line),
                                text: change.to_string_lossy().to_string(),
                                tag: ChangeTag::Equal,
                            }),
                            right: Some(DiffLine {
                                line_no: Some(right_line),
                                text: change.to_string_lossy().to_string(),
                                tag: ChangeTag::Equal,
                            }),
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
                        rows.push(DiffRow { left, right });
                    }
                }
                similar::DiffTag::Delete => {
                    for change in diff.iter_changes(op) {
                        rows.push(DiffRow {
                            left: Some(DiffLine {
                                line_no: Some(left_line),
                                text: change.to_string_lossy().to_string(),
                                tag: ChangeTag::Delete,
                            }),
                            right: None,
                        });
                        left_line += 1;
                    }
                }
                similar::DiffTag::Insert => {
                    for change in diff.iter_changes(op) {
                        rows.push(DiffRow {
                            left: None,
                            right: Some(DiffLine {
                                line_no: Some(right_line),
                                text: change.to_string_lossy().to_string(),
                                tag: ChangeTag::Insert,
                            }),
                        });
                        right_line += 1;
                    }
                }
            }
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
            assert!(row.left.is_some());
            assert!(row.right.is_some());
            assert_eq!(row.left.as_ref().unwrap().tag, ChangeTag::Equal);
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
                r.right
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
                r.left
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
        // Should have a delete on left and insert on right
        let has_replace = rows.iter().any(|r| {
            r.left
                .as_ref()
                .map(|d| d.tag == ChangeTag::Delete)
                .unwrap_or(false)
                && r.right
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
        // Check that line numbers are sequential
        let left_nums: Vec<usize> = rows
            .iter()
            .filter_map(|r| r.left.as_ref().and_then(|d| d.line_no))
            .collect();
        for w in left_nums.windows(2) {
            assert!(w[1] >= w[0]);
        }
    }
}
