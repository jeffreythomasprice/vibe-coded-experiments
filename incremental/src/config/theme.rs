use crate::resource::{RESOURCE_COUNT, Resource};

pub struct ToastTheme {
    pub colors: [&'static str; RESOURCE_COUNT],
    pub outlines: [&'static str; RESOURCE_COUNT],
    pub outline_width: f64,
    pub font_size: f64,
    pub duration: f64,
    pub fade: f64,
    pub rise: f64,
}

impl ToastTheme {
    pub fn color(&self, r: Resource) -> &'static str {
        self.colors[r.index()]
    }

    pub fn outline(&self, r: Resource) -> &'static str {
        self.outlines[r.index()]
    }
}

pub struct Theme {
    pub background: &'static str,
    pub empty_fill: &'static str,
    pub owned_fill: &'static str,
    pub selected_fill: &'static str,
    pub empty_border: &'static str,
    pub owned_border: &'static str,
    pub selected_border: &'static str,
    pub panel_background: &'static str,
    pub panel_border: &'static str,
    pub panel_text: &'static str,
    pub bar_background: &'static str,
    pub bar_generate: &'static str,
    pub bar_overfill: &'static str,
    pub border_width: f64,
    pub selected_border_width: f64,
    pub toast: ToastTheme,
}

pub const DARK: Theme = Theme {
    background: "#101018",
    empty_fill: "#16161f",
    owned_fill: "#2f4a6b",
    selected_fill: "#4a6f9c",
    empty_border: "#23232f",
    owned_border: "#8fb4e0",
    selected_border: "#ffd866",
    panel_background: "#181824",
    panel_border: "#8fb4e0",
    panel_text: "#d8dde6",
    bar_background: "#23232f",
    bar_generate: "#4a6f9c",
    bar_overfill: "#c9a227",
    border_width: 1.0,
    selected_border_width: 3.0,
    toast: ToastTheme {
        colors: ["#7ed46a", "#ff6e40", "#d9ad5b", "#d5dde3", "#4fc3f7"],
        outlines: ["#0e2a0a", "#3a0d05", "#2e1f08", "#1c2328", "#062638"],
        outline_width: 3.0,
        font_size: 18.0,
        duration: 1.0,
        fade: 0.6,
        rise: 24.0,
    },
};
