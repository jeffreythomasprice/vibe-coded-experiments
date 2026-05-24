//! Read-only "● ● ○ ○ ○" rendering of a rated trait. Used by the derived
//! sidebar; editable variants live in `rated_trait`.

pub fn dots(ui: &mut egui::Ui, filled: u8, max: u8) {
    let mut s = String::with_capacity((max as usize) * 2);
    for i in 0..max {
        if i < filled {
            s.push('●');
        } else {
            s.push('○');
        }
        if i + 1 < max {
            s.push(' ');
        }
    }
    ui.label(s);
}
