//! Interactive row (or grid) of clickable dots for editing a dot-rated trait.
//!
//! Click an empty dot N to set the total to N; click a filled dot N to drop the
//! total to N − 1. Returns the desired new total, already clamped to
//! `[min, max]`, or `None` if nothing was clicked.

use egui::{vec2, Color32, Response, Sense, Stroke};

pub struct DotsGrid {
    current: u8,
    max: u8,
    min: u8,
    per_row: u8,
}

impl DotsGrid {
    pub fn new(current: u8, max: u8) -> Self {
        Self {
            current,
            max,
            min: 0,
            per_row: 10,
        }
    }

    /// Floor for the returned total. Clicks that resolve below `min` snap to
    /// `min` (so dots below `min` behave as a no-op while still rendering).
    pub fn min(mut self, min: u8) -> Self {
        self.min = min;
        self
    }

    pub fn per_row(mut self, per_row: u8) -> Self {
        self.per_row = per_row.max(1);
        self
    }

    pub fn show(self, ui: &mut egui::Ui, _id_source: impl std::hash::Hash) -> Option<u8> {
        let Self {
            current,
            max,
            min,
            per_row,
        } = self;

        let visuals = ui.style().visuals.clone();
        let filled_color = visuals.widgets.active.fg_stroke.color;
        let base_color = filled_color.gamma_multiply(0.55);
        let stroke = Stroke::new(1.0, visuals.widgets.inactive.fg_stroke.color);

        let size = 14.0_f32;
        let radius = size * 0.4;

        let mut new_total: Option<u8> = None;
        let mut row_response: Option<Response> = None;

        let render_row = |ui: &mut egui::Ui, range: std::ops::Range<u8>| -> (Response, Option<u8>) {
            let mut local_resp: Option<Response> = None;
            let mut clicked_to: Option<u8> = None;
            ui.horizontal(|ui| {
                for i in range {
                    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
                    let center = rect.center();
                    let is_filled = i < current;
                    let is_base = i < min;
                    let color = if is_filled {
                        if is_base {
                            base_color
                        } else {
                            filled_color
                        }
                    } else {
                        Color32::TRANSPARENT
                    };
                    if is_filled {
                        ui.painter().circle(center, radius, color, stroke);
                    } else {
                        ui.painter().circle_stroke(center, radius, stroke);
                    }
                    if resp.clicked() {
                        // Click on filled dot N -> total = N (unfill this dot and above).
                        // Click on empty dot N -> total = N+1 (fill 1..=N+1).
                        let target = if is_filled {
                            (i as u16).max(min as u16) as u8
                        } else {
                            ((i as u16) + 1).min(max as u16) as u8
                        };
                        if target != current {
                            clicked_to = Some(target);
                        }
                    }
                    local_resp = Some(match local_resp.take() {
                        Some(prev) => prev.union(resp),
                        None => resp,
                    });
                }
            });
            (local_resp.expect("at least one dot per row"), clicked_to)
        };

        if max <= per_row {
            let (resp, clicked) = render_row(ui, 0..max);
            new_total = clicked;
            row_response = Some(resp);
        } else {
            ui.vertical(|ui| {
                let mut start: u8 = 0;
                while start < max {
                    let end = (start as u16 + per_row as u16).min(max as u16) as u8;
                    let (resp, clicked) = render_row(ui, start..end);
                    if clicked.is_some() {
                        new_total = clicked;
                    }
                    row_response = Some(match row_response.take() {
                        Some(prev) => prev.union(resp),
                        None => resp,
                    });
                    start = end;
                }
            });
        }

        if let Some(resp) = row_response {
            resp.on_hover_text(format!("{} / {}", current, max));
        }

        new_total
    }
}
