use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use similar::ChangeTag;

use super::diff_model::DiffRow;
use super::styles;

pub fn render_pane(rows: &[DiffRow], scroll: usize, is_left: bool) -> Paragraph<'_> {
    let lines: Vec<Line> = rows
        .iter()
        .skip(scroll)
        .map(|row| {
            let entry = if is_left { &row.left } else { &row.right };
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
                None => Line::from(Span::raw("")),
            }
        })
        .collect();

    Paragraph::new(lines)
}
