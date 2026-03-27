use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use similar::ChangeTag;

use super::diff_model::DiffRow;
use super::styles;

pub fn render_pane(rows: &[DiffRow], scroll: usize, column_index: usize, word_wrap: bool) -> Paragraph<'_> {
    let lines: Vec<Line> = rows
        .iter()
        .skip(scroll)
        .map(|row| {
            let entry = row.columns.get(column_index).and_then(|c| c.as_ref());
            match entry {
                Some(diff_line) => {
                    let line_no = diff_line
                        .line_no
                        .map(|n| format!("{:4} ", n))
                        .unwrap_or_else(|| "     ".to_string());

                    let text = diff_line.text.trim_end().to_string();

                    let style = match diff_line.tag {
                        ChangeTag::Equal => styles::EQUAL,
                        ChangeTag::Delete => styles::DELETE,
                        ChangeTag::Insert => styles::INSERT,
                    };

                    Line::from(vec![
                        Span::styled(line_no, styles::LINE_NUMBER),
                        Span::styled(text, style),
                    ])
                }
                None => Line::from(Span::styled("", styles::EQUAL)),
            }
        })
        .collect();

    let para = Paragraph::new(lines);
    if word_wrap {
        para.wrap(Wrap { trim: false })
    } else {
        para
    }
}
