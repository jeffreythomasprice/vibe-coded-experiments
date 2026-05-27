use std::path::Path;

use crate::Config;
use crate::character::{BackgroundRef, CharmRef, SpellRef};
use crate::render::pdf::character_to_pdf;
use crate::ui::dialogs::confirm_discard::{self, DiscardChoice};
use crate::ui::dialogs::file_picker;
use crate::ui::io::{load_character_from_path, save_character_to_path};
use crate::ui::menu::{self, MenuAction};
use crate::ui::persisted::{PanelLocation, UiState};
use crate::ui::pickers::{PickerOutcome, background_picker, charm_picker, spell_picker};
use crate::ui::search::{self, SearchBarOutcome};
use crate::ui::sections;
use crate::ui::sidebar;
use crate::ui::state::{AppState, PendingAction, StartupAction, StatusKind, blank_character};
use crate::ui::widgets::custom_entry::{background_entry_form, charm_entry_form, spell_entry_form};

pub struct App {
    state: AppState,
}

impl App {
    pub fn new(ctx: &egui::Context, start: StartupAction, config: Config) -> Self {
        let ui_state = UiState::load_or_default(config.state_file);
        ctx.set_theme(egui::ThemePreference::from(ui_state.theme_preference));
        let mut state = AppState::new_with_state(ui_state);
        match start {
            StartupAction::Empty => {
                tracing::info!("starting GUI editor with blank document");
            }
            StartupAction::OpenFile(path) => {
                tracing::info!(path = %path.display(), "starting GUI editor with file");
                match load_character_from_path(&path) {
                    Ok(c) => {
                        let abs = std::fs::canonicalize(&path).unwrap_or(path);
                        state.character = c;
                        state.last_dir = abs.parent().map(Path::to_path_buf);
                        state.file_path = Some(abs);
                        state.dirty = false;
                        state.validation_dirty = true;
                    }
                    Err(e) => {
                        state.status_message = Some((StatusKind::Error, e.to_string()));
                    }
                }
            }
        }
        Self { state }
    }

    fn current_file_label(&self) -> String {
        match &self.state.file_path {
            Some(p) => p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string()),
            None => "Untitled".to_string(),
        }
    }

    fn current_title(&self) -> String {
        let dirty = if self.state.dirty { "* " } else { "" };
        format!("{}{} — Exalted", dirty, self.current_file_label())
    }

    fn dispatch_menu(&mut self, ctx: &egui::Context, action: MenuAction) {
        match action {
            MenuAction::New => {
                if self.state.dirty {
                    self.state.pending_action = Some(PendingAction::NewDocument);
                } else {
                    self.do_new();
                }
            }
            MenuAction::Open => {
                if self.state.dirty {
                    self.state.pending_action = Some(PendingAction::OpenExisting);
                } else {
                    self.do_open();
                }
            }
            MenuAction::Save => {
                self.do_save();
            }
            MenuAction::SaveAs => {
                self.do_save_as();
            }
            MenuAction::ExportPdf => {
                self.do_export_pdf();
            }
            MenuAction::Quit => {
                if self.state.dirty {
                    self.state.pending_action = Some(PendingAction::Quit);
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            MenuAction::Find => {
                self.state.search.open();
            }
            MenuAction::ToggleSidebar => {
                self.state.ui_state.derived_visible = !self.state.ui_state.derived_visible;
                tracing::debug!(
                    panel = "derived",
                    visible = self.state.ui_state.derived_visible,
                    "panel visibility toggled"
                );
                self.state.ui_state.save();
            }
            MenuAction::ToggleValidationPanel => {
                self.state.ui_state.validation_visible = !self.state.ui_state.validation_visible;
                tracing::debug!(
                    panel = "validation",
                    visible = self.state.ui_state.validation_visible,
                    "panel visibility toggled"
                );
                self.state.ui_state.save();
            }
            MenuAction::ToggleActionsPanel => {
                self.state.ui_state.actions_visible = !self.state.ui_state.actions_visible;
                tracing::debug!(
                    panel = "actions",
                    visible = self.state.ui_state.actions_visible,
                    "panel visibility toggled"
                );
                self.state.ui_state.save();
            }
            MenuAction::ToggleDiceLogPanel => {
                self.state.ui_state.dicelog_visible = !self.state.ui_state.dicelog_visible;
                tracing::debug!(
                    panel = "dicelog",
                    visible = self.state.ui_state.dicelog_visible,
                    "panel visibility toggled"
                );
                self.state.ui_state.save();
            }
            MenuAction::SetTheme(pref) => {
                self.state.ui_state.set_theme_preference(pref);
                ctx.set_theme(egui::ThemePreference::from(pref));
                self.state.ui_state.save();
            }
        }
    }

    fn do_new(&mut self) {
        tracing::info!("new blank document");
        self.state.character = blank_character();
        self.state.file_path = None;
        self.state.dirty = false;
        self.state.validation_dirty = true;
        self.state.status_message = None;
    }

    fn do_open(&mut self) {
        let Some(path) = file_picker::open_character(self.state.last_dir.as_deref()) else {
            tracing::debug!("open dialog cancelled");
            return;
        };
        match load_character_from_path(&path) {
            Ok(c) => {
                self.state.character = c;
                self.state.last_dir = path.parent().map(Path::to_path_buf);
                self.state.file_path = Some(path);
                self.state.dirty = false;
                self.state.validation_dirty = true;
                self.state.status_message = None;
            }
            Err(e) => {
                self.state.status_message = Some((StatusKind::Error, e.to_string()));
            }
        }
    }

    /// Attempt to save to the current `file_path`. If there isn't one,
    /// falls through to Save As. Returns true if the file was written.
    fn do_save(&mut self) -> bool {
        let Some(path) = self.state.file_path.clone() else {
            return self.do_save_as();
        };
        self.write_to(&path)
    }

    fn do_save_as(&mut self) -> bool {
        let suggested = self
            .state
            .file_path
            .as_ref()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "character.toml".to_string());
        let Some(path) =
            file_picker::save_character(self.state.last_dir.as_deref(), Some(&suggested))
        else {
            tracing::debug!("save-as dialog cancelled");
            return false;
        };
        if self.write_to(&path) {
            self.state.last_dir = path.parent().map(Path::to_path_buf);
            self.state.file_path = Some(path);
            true
        } else {
            false
        }
    }

    fn write_to(&mut self, path: &Path) -> bool {
        match save_character_to_path(&self.state.character, path) {
            Ok(()) => {
                self.state.dirty = false;
                self.state.status_message =
                    Some((StatusKind::Info, format!("Saved {}", path.display())));
                true
            }
            Err(e) => {
                self.state.status_message = Some((StatusKind::Error, e.to_string()));
                false
            }
        }
    }

    fn do_export_pdf(&mut self) {
        let suggested = self
            .state
            .file_path
            .as_ref()
            .and_then(|p| {
                p.file_stem()
                    .map(|s| format!("{}.pdf", s.to_string_lossy()))
            })
            .unwrap_or_else(|| "character.pdf".to_string());
        let start_dir = self
            .state
            .file_path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| self.state.last_dir.clone());
        let Some(path) = file_picker::export_pdf(start_dir.as_deref(), Some(&suggested)) else {
            tracing::debug!("export-pdf dialog cancelled");
            return;
        };
        tracing::info!(path = %path.display(), "exporting PDF");
        let bytes = match character_to_pdf(&self.state.character) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(error = %e, "PDF render failed");
                self.state.status_message =
                    Some((StatusKind::Error, format!("PDF render failed: {}", e)));
                return;
            }
        };
        match std::fs::write(&path, &bytes) {
            Ok(()) => {
                tracing::info!(path = %path.display(), bytes = bytes.len(), "exported PDF");
                self.state.last_dir = path.parent().map(Path::to_path_buf);
                self.state.status_message =
                    Some((StatusKind::Info, format!("Exported {}", path.display())));
            }
            Err(e) => {
                tracing::error!(path = %path.display(), error = %e, "could not write PDF");
                self.state.status_message = Some((
                    StatusKind::Error,
                    format!("could not write {}: {}", path.display(), e),
                ));
            }
        }
    }

    /// Drive the confirm-discard modal when a `pending_action` is waiting on
    /// the user's decision about unsaved changes.
    fn handle_pending_action(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.state.pending_action else {
            return;
        };

        // If the doc is no longer dirty (e.g. an in-flight Save succeeded),
        // run the pending action immediately without prompting.
        if !self.state.dirty {
            self.state.pending_action = None;
            self.perform_pending(ctx, pending);
            return;
        }

        let label = self.current_file_label();
        let Some(choice) = confirm_discard::show(ctx, &label) else {
            return;
        };
        tracing::debug!(choice = ?choice, "discard dialog resolved");
        match choice {
            DiscardChoice::Save => {
                if self.do_save() {
                    self.state.pending_action = None;
                    self.perform_pending(ctx, pending);
                }
                // If save failed or was cancelled, leave pending_action set
                // and let the modal re-render next frame.
            }
            DiscardChoice::Discard => {
                self.state.dirty = false;
                self.state.pending_action = None;
                self.perform_pending(ctx, pending);
            }
            DiscardChoice::Cancel => {
                self.state.pending_action = None;
            }
        }
    }

    fn perform_pending(&mut self, ctx: &egui::Context, pending: PendingAction) {
        match pending {
            PendingAction::NewDocument => self.do_new(),
            PendingAction::OpenExisting => self.do_open(),
            PendingAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Push title changes when they happen (not every frame).
        let title = self.current_title();
        if title != self.state.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.state.last_title = title;
        }

        // Intercept the window-close request when the document is dirty so we
        // can route through the confirm-discard modal first.
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && self.state.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.state.pending_action.is_none() {
                self.state.pending_action = Some(PendingAction::Quit);
            }
        }

        // Keyboard shortcuts first so they can't be eaten by inputs that
        // happen to be focused when the user hits Ctrl-S.
        if let Some(action) = menu::shortcut_action(&ctx) {
            self.dispatch_menu(&ctx, action);
        }

        egui::Panel::top("menu_bar").show_inside(ui, |ui| {
            if let Some(action) = menu::render(ui, self.state.ui_state.theme_preference) {
                self.dispatch_menu(&ctx, action);
            }
        });

        let mut clear_status = false;
        if let Some((kind, msg)) = self.state.status_message.clone() {
            let bg = match kind {
                StatusKind::Info => egui::Color32::from_rgb(40, 60, 40),
                StatusKind::Error => egui::Color32::from_rgb(70, 30, 30),
            };
            egui::Panel::top("status_bar")
                .frame(egui::Frame::default().fill(bg).inner_margin(6.0))
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(msg);
                        if ui.button("dismiss").clicked() {
                            clear_status = true;
                        }
                    });
                });
        }
        if clear_status {
            self.state.status_message = None;
        }

        // Search: handle close shortcut and (re)compute matches before
        // sections render so highlight queries this frame are accurate.
        // Enter / Shift+Enter nav is handled inside the search bar itself
        // so it only fires while the search input has focus.
        if self.state.search.visible {
            ctx.input_mut(|i| {
                use egui::{Key, Modifiers};
                if i.consume_key(Modifiers::NONE, Key::Escape) {
                    self.state.search.close();
                }
            });
            self.state.search.recompute(&self.state.character);
        }

        // Dockable panels. Left & right are rendered before the bottom so
        // the bottom panel spans the full window width.
        sidebar::panel::render_location(ui, PanelLocation::Left, &mut self.state);
        sidebar::panel::render_location(ui, PanelLocation::Right, &mut self.state);
        sidebar::panel::render_location(ui, PanelLocation::Bottom, &mut self.state);

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    sections::render_all(ui, &mut self.state);
                });
        });

        // Search bar floats on top of everything below the menu/status bars.
        let outcome: SearchBarOutcome = search::render_search_bar(&ctx, &mut self.state.search);
        if outcome.close {
            self.state.search.close();
        } else {
            if outcome.next {
                self.state.search.next();
            }
            if outcome.prev {
                self.state.search.prev();
            }
        }

        // One-shot: scroll requests from this frame have been consumed by the
        // widgets that called scroll_to_me.
        self.state.search.scroll_pending = false;

        self.drive_pickers(&ctx);
        self.drive_edit_modals(&ctx);

        // Confirm-discard modal goes last so it draws above the pickers.
        self.handle_pending_action(&ctx);
    }
}

enum EditOutcome {
    Stay,
    Saved,
    Cancelled,
}

impl App {
    fn drive_edit_modals(&mut self, ctx: &egui::Context) {
        // Background.
        if let Some((idx, mut entry)) = self.state.editing_custom_background.take() {
            let outcome = show_edit_modal(ctx, "Edit custom background", "edit-custom-bg", |ui| {
                background_entry_form(ui, "edit-custom-bg", &mut entry);
                !entry.id.is_empty() && !entry.name.is_empty()
            });
            match outcome {
                EditOutcome::Stay => {
                    self.state.editing_custom_background = Some((idx, entry));
                }
                EditOutcome::Cancelled => {
                    tracing::debug!(
                        kind = "background",
                        index = idx,
                        "custom entry edit cancelled"
                    );
                }
                EditOutcome::Saved => {
                    if let Some(BackgroundRef::Custom { entry: target, .. }) =
                        self.state.character.backgrounds.get_mut(idx)
                    {
                        *target = entry;
                        tracing::debug!(
                            kind = "background",
                            index = idx,
                            "custom entry edit saved"
                        );
                        self.state.mark_dirty_with("custom.background.edit");
                    }
                }
            }
        }

        // Charm.
        if let Some((idx, mut entry)) = self.state.editing_custom_charm.take() {
            let outcome = show_edit_modal(ctx, "Edit custom charm", "edit-custom-charm", |ui| {
                charm_entry_form(ui, "edit-custom-charm", &mut entry);
                !entry.id.is_empty() && !entry.name.is_empty()
            });
            match outcome {
                EditOutcome::Stay => {
                    self.state.editing_custom_charm = Some((idx, entry));
                }
                EditOutcome::Cancelled => {
                    tracing::debug!(kind = "charm", index = idx, "custom entry edit cancelled");
                }
                EditOutcome::Saved => {
                    if let Some(CharmRef::Custom { entry: target, .. }) =
                        self.state.character.charms.get_mut(idx)
                    {
                        *target = entry;
                        tracing::debug!(kind = "charm", index = idx, "custom entry edit saved");
                        self.state.mark_dirty_with("custom.charm.edit");
                    }
                }
            }
        }

        // Spell.
        if let Some((idx, mut entry)) = self.state.editing_custom_spell.take() {
            let outcome = show_edit_modal(ctx, "Edit custom spell", "edit-custom-spell", |ui| {
                spell_entry_form(ui, "edit-custom-spell", &mut entry);
                !entry.id.is_empty() && !entry.name.is_empty()
            });
            match outcome {
                EditOutcome::Stay => {
                    self.state.editing_custom_spell = Some((idx, entry));
                }
                EditOutcome::Cancelled => {
                    tracing::debug!(kind = "spell", index = idx, "custom entry edit cancelled");
                }
                EditOutcome::Saved => {
                    if let Some(SpellRef::Custom { entry: target, .. }) =
                        self.state.character.spells.get_mut(idx)
                    {
                        *target = entry;
                        tracing::debug!(kind = "spell", index = idx, "custom entry edit saved");
                        self.state.mark_dirty_with("custom.spell.edit");
                    }
                }
            }
        }
    }

    fn drive_pickers(&mut self, ctx: &egui::Context) {
        // Borrow the picker state out, run the picker, then decide whether to
        // close it. This avoids holding &mut on self.state while the picker
        // also wants &mut state.character or &state.character.
        if let Some(mut picker) = self.state.charm_picker.take() {
            let outcome = charm_picker::show(ctx, &mut picker, &self.state.character);
            match outcome {
                PickerOutcome::Stay => {
                    self.state.charm_picker = Some(picker);
                }
                PickerOutcome::Cancelled => {
                    tracing::debug!(target: "exalted::ui::picker", picker = "charm", outcome = "cancelled", "closed picker");
                }
                PickerOutcome::Picked(c) => {
                    tracing::debug!(target: "exalted::ui::picker", picker = "charm", outcome = "picked", "closed picker");
                    self.state.character.charms.push(c);
                    self.state.mark_dirty_with("picker.charm.confirm");
                }
            }
        }
        if let Some(mut picker) = self.state.spell_picker.take() {
            let outcome = spell_picker::show(ctx, &mut picker, &self.state.character);
            match outcome {
                PickerOutcome::Stay => {
                    self.state.spell_picker = Some(picker);
                }
                PickerOutcome::Cancelled => {
                    tracing::debug!(target: "exalted::ui::picker", picker = "spell", outcome = "cancelled", "closed picker");
                }
                PickerOutcome::Picked(s) => {
                    tracing::debug!(target: "exalted::ui::picker", picker = "spell", outcome = "picked", "closed picker");
                    self.state.character.spells.push(s);
                    self.state.mark_dirty_with("picker.spell.confirm");
                }
            }
        }
        if let Some(mut picker) = self.state.background_picker.take() {
            let outcome = background_picker::show(ctx, &mut picker);
            match outcome {
                PickerOutcome::Stay => {
                    self.state.background_picker = Some(picker);
                }
                PickerOutcome::Cancelled => {
                    tracing::debug!(target: "exalted::ui::picker", picker = "background", outcome = "cancelled", "closed picker");
                }
                PickerOutcome::Picked(b) => {
                    tracing::debug!(target: "exalted::ui::picker", picker = "background", outcome = "picked", "closed picker");
                    self.state.character.backgrounds.push(b);
                    self.state.mark_dirty_with("picker.background.confirm");
                }
            }
        }
    }
}

fn show_edit_modal(
    ctx: &egui::Context,
    title: &str,
    id_salt: &str,
    body: impl FnOnce(&mut egui::Ui) -> bool,
) -> EditOutcome {
    let mut outcome = EditOutcome::Stay;
    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_size([640.0, 620.0])
        .show(ctx, |ui| {
            let body_height = (ui.available_height() - 60.0).max(120.0);
            let mut can_save = false;
            egui::ScrollArea::vertical()
                .id_salt((id_salt, "scroll"))
                .auto_shrink([false; 2])
                .max_height(body_height)
                .show(ui, |ui| {
                    can_save = body(ui);
                });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    outcome = EditOutcome::Cancelled;
                }
                if ui
                    .add_enabled(can_save, egui::Button::new("Save"))
                    .clicked()
                {
                    outcome = EditOutcome::Saved;
                }
            });
        });
    outcome
}
