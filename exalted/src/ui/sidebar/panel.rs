//! Tabbed dock orchestrator.
//!
//! Each call to [`render_location`] paints one of the three dockable
//! locations (left sidebar, right sidebar, bottom panel) with whichever
//! [`PanelItem`]s are currently assigned to it and visible. The tab strip
//! at the top supports:
//!
//! - **Left click on a tab** — make that tab the active one.
//! - **Middle click on a tab** — hide that tab (the View menu can restore
//!   it later; on restore the item returns to this location).
//! - **Right click on a tab** — context menu for moving the tab to another
//!   location.
//! - **✕ button at the right end of the strip** — hide the *active* tab
//!   (same behavior as the previous per-panel close button).
//!
//! When no visible items remain for a location, the whole `Panel` is
//! skipped so the central area expands to fill the space.

use crate::ui::persisted::{PanelItem, PanelLocation};
use crate::ui::sidebar::{derived, validation};
use crate::ui::state::AppState;

pub fn render_location(ui: &mut egui::Ui, location: PanelLocation, state: &mut AppState) {
    let items: Vec<PanelItem> = PanelItem::ORDER
        .iter()
        .copied()
        .filter(|item| {
            state.ui_state.is_visible(*item) && state.ui_state.location_of(*item) == location
        })
        .collect();
    if items.is_empty() {
        return;
    }

    let active = resolve_active(state, location, &items);

    let panel_id = format!("dock_panel::{:?}", location);
    let render =
        |ui: &mut egui::Ui, state: &mut AppState| render_inner(ui, state, location, &items, active);

    match location {
        PanelLocation::Left => {
            egui::Panel::left(panel_id)
                .resizable(true)
                .default_size(260.0)
                .min_size(180.0)
                .show_inside(ui, |ui| render(ui, state));
        }
        PanelLocation::Right => {
            egui::Panel::right(panel_id)
                .resizable(true)
                .default_size(260.0)
                .min_size(180.0)
                .show_inside(ui, |ui| render(ui, state));
        }
        PanelLocation::Bottom => {
            egui::Panel::bottom(panel_id)
                .resizable(true)
                .default_size(180.0)
                .min_size(80.0)
                .show_inside(ui, |ui| render(ui, state));
        }
    }
}

fn resolve_active(state: &mut AppState, location: PanelLocation, items: &[PanelItem]) -> PanelItem {
    let current = state.ui_state.active(location);
    if let Some(item) = current {
        if items.contains(&item) {
            return item;
        }
    }
    let fallback = items[0];
    if current != Some(fallback) {
        state.ui_state.set_active(location, Some(fallback));
        state.ui_state.save();
    }
    fallback
}

fn render_inner(
    ui: &mut egui::Ui,
    state: &mut AppState,
    location: PanelLocation,
    items: &[PanelItem],
    active: PanelItem,
) {
    let mut new_active: Option<PanelItem> = None;
    let mut hide_item: Option<PanelItem> = None;
    let mut move_item: Option<(PanelItem, PanelLocation)> = None;
    let mut close_active = false;

    ui.horizontal(|ui| {
        for &item in items {
            let resp = ui.selectable_label(item == active, item.label());
            if resp.clicked() {
                new_active = Some(item);
            }
            if resp.middle_clicked() {
                hide_item = Some(item);
            }
            resp.context_menu(|ui| {
                ui.label("Move to");
                for loc in PanelLocation::ALL {
                    let enabled = loc != location;
                    if ui
                        .add_enabled(enabled, egui::Button::new(loc.label()))
                        .clicked()
                    {
                        move_item = Some((item, loc));
                        ui.close();
                    }
                }
            });
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("✕")
                .on_hover_text(format!("Hide {}", active.label()))
                .clicked()
            {
                close_active = true;
            }
        });
    });
    ui.separator();

    let body_id = ("dock_body", location, active);
    egui::ScrollArea::vertical()
        .id_salt(body_id)
        .auto_shrink([false; 2])
        .show(ui, |ui| match active {
            PanelItem::Derived => derived::render_body(ui, &state.character),
            PanelItem::Validation => validation::render_body(ui, state),
        });

    let mut needs_save = false;
    if let Some(item) = new_active {
        state.ui_state.set_active(location, Some(item));
        needs_save = true;
    }
    if let Some(item) = hide_item {
        state.ui_state.set_visible(item, false);
        needs_save = true;
    }
    if close_active {
        state.ui_state.set_visible(active, false);
        needs_save = true;
    }
    if let Some((item, dest)) = move_item {
        state.ui_state.set_location(item, dest);
        // Make the moved item the active tab in its new home so the user
        // immediately sees the result of their menu choice.
        state.ui_state.set_active(dest, Some(item));
        needs_save = true;
    }
    if needs_save {
        state.ui_state.save();
    }
}
