//! Layout and painting of the HUD and menu screens. Everything is placed
//! explicitly on the theme's pixel grid.

use common::Corner;
use egui::{Id, Order, Pos2, Rect, Vec2};

use crate::menu::{
    clear_binding, entries, Entry, MenuState, Screen, BINDING_SLOTS, MAX_STATE_NAME,
};
use crate::nav::{scroll_to, Cursor};
use crate::theme::Theme;
use crate::widgets::{self, Arrow, CURSOR_WIDTH, DIALOG_PADDING, ROW_HEIGHT};
use crate::{trigger_label, MenuCommand, MenuData};

/// Gap between a dialog and the screen edge or another dialog, in UI pixels.
const MARGIN: f32 = 4.0;
/// Preferred content width of the bindings screen, in UI pixels.
const BINDINGS_WIDTH: f32 = 272.0;
/// Width of the clickable `x` after a filled binding cell, in UI pixels.
const CLEAR_WIDTH: f32 = 12.0;
/// How long the speed box takes to change opacity.
const FADE_SECONDS: f32 = 0.25;
/// Wheel travel, in points, that moves the cursor one entry.
const SCROLL_STEP: f32 = 40.0;

const BINDINGS_HINT: &str = "A: Rebind  B: Back  Del: Clear";
const CAPTURE_HINT: &str = "Press any input to bind it.";
const NAMING_HINT: &str = "Enter: Save  Esc: Cancel";
const NO_ROMS: &str = "No ROMs found.";
const NO_STATES: &str = "No saves yet.";

/// The always-visible HUD: the speed box in its configured corner and, when
/// paused, a `Paused` box top-center. Neither takes pointer input. Returns the
/// bottom edge of the lowest box along the top, which the menu stays below.
pub(crate) fn draw_hud(ctx: &egui::Context, theme: &Theme, data: &MenuData) -> f32 {
    let screen = ctx.screen_rect();
    let mut lines = vec![format!("Speed: {}", data.speed_label)];
    if let Some(actual) = data.actual_speed {
        lines.push(format!("Actual: {actual:.2}x"));
    }
    let size = text_box_size(ctx, theme, &lines);
    let area = screen.shrink(theme.px(MARGIN));
    let (right, at_bottom) = match data.speed_corner {
        Corner::TopLeft => (false, false),
        Corner::TopRight => (true, false),
        Corner::BottomLeft => (false, true),
        Corner::BottomRight => (true, true),
    };
    let min = Pos2::new(
        if right {
            area.min.x + theme.snap(area.width() - size.x)
        } else {
            area.min.x
        },
        if at_bottom {
            area.min.y + theme.snap(area.height() - size.y)
        } else {
            area.min.y
        },
    );
    let speed = Rect::from_min_size(min, size);
    let opacity = ctx.animate_value_with_time(
        Id::new("hud-speed-opacity"),
        data.speed_opacity,
        FADE_SECONDS,
    );
    text_box(ctx, theme, "hud-speed", speed, opacity, &lines);
    // The speed box, if it is one the other top boxes have to make room for.
    let top_speed = (!at_bottom).then_some(speed);
    let mut bottom = top_speed.map_or(screen.min.y, |rect| rect.max.y);

    if data.paused {
        let lines = ["Paused".to_owned()];
        let size = text_box_size(ctx, theme, &lines);
        let x = screen.min.x + theme.snap((screen.width() - size.x) / 2.0);
        let mut paused = Rect::from_min_size(Pos2::new(x, area.min.y), size);
        // Below the speed box when the window is too narrow to center beside it.
        if let Some(speed) = top_speed {
            if paused.expand(theme.px(MARGIN) - 0.5).intersects(speed) {
                paused =
                    paused.translate(Vec2::new(0.0, speed.max.y + theme.px(MARGIN) - area.min.y));
            }
        }
        text_box(ctx, theme, "hud-paused", paused, 1.0, &lines);
        bottom = bottom.max(paused.max.y);
    }

    if let Some(toast) = data.toast {
        let lines = [toast.to_owned()];
        let mut size = text_box_size(ctx, theme, &lines);
        size.x = size.x.min(area.width());
        // With nothing along the top, the toast takes the top edge itself.
        let y = if bottom > screen.min.y {
            bottom + theme.px(MARGIN)
        } else {
            area.min.y
        };
        let rect = Rect::from_min_size(Pos2::new(area.min.x, y), size);
        text_box(ctx, theme, "hud-toast", rect, 1.0, &lines);
        bottom = rect.max.y;
    }
    bottom
}

fn text_box_size(ctx: &egui::Context, theme: &Theme, lines: &[String]) -> Vec2 {
    let widest = lines
        .iter()
        .map(|line| widgets::text_width(ctx, theme, line))
        .fold(0.0, f32::max);
    let padding = theme.px(2.0 * DIALOG_PADDING);
    Vec2::new(
        theme.snap(widest) + theme.px(1.0) + padding,
        theme.px(lines.len() as f32 * ROW_HEIGHT) + padding,
    )
}

fn text_box(
    ctx: &egui::Context,
    theme: &Theme,
    id: &str,
    rect: Rect,
    opacity: f32,
    lines: &[String],
) {
    widgets::dialog_box(
        ctx,
        theme,
        Id::new(id),
        Order::Middle,
        false,
        rect,
        opacity,
        |ui, inner| {
            for (index, line) in lines.iter().enumerate() {
                let pos = inner.min + Vec2::new(0.0, theme.px(index as f32 * ROW_HEIGHT));
                widgets::text(ui.painter(), theme, pos, inner.width(), line);
            }
        },
    );
}

/// Draw the open menu screen and apply any mouse interaction with it.
pub(crate) fn draw_menu(
    ctx: &egui::Context,
    theme: &Theme,
    state: &mut MenuState,
    data: &MenuData,
    hud_bottom: f32,
    commands: &mut Vec<MenuCommand>,
) {
    let Some(screen) = state.screen() else {
        return;
    };
    let rows = entries(screen, data);
    state.cursor = state
        .cursor
        .clamped(rows.len(), |row| rows.get(row).map_or(0, Vec::len));
    scroll_with_wheel(ctx, state, &rows, data, commands);

    let hint = match screen {
        _ if state.is_naming() => Some(NAMING_HINT),
        Screen::InputBindings if state.is_capturing() => Some(CAPTURE_HINT),
        Screen::InputBindings => Some(BINDINGS_HINT),
        _ => None,
    };

    // The space left for the dialog between the HUD and the hint box.
    let mut space = ctx.screen_rect().shrink(theme.px(MARGIN));
    space.min.y = hud_bottom + theme.px(MARGIN);
    if let Some(hint) = hint {
        let lines = [hint.to_owned()];
        let height = text_box_size(ctx, theme, &lines).y;
        let rect = Rect::from_min_max(Pos2::new(space.min.x, space.max.y - height), space.max);
        widgets::dialog_box(
            ctx,
            theme,
            Id::new("menu-hint"),
            Order::Foreground,
            true,
            rect,
            1.0,
            |ui, inner| widgets::text(ui.painter(), theme, inner.min, inner.width(), hint),
        );
        space.max.y = rect.min.y - theme.px(MARGIN);
    }

    if let Some(name) = state.name_being_typed().map(str::to_owned) {
        draw_name_field(ctx, theme, state, data, &name, space, commands);
        return;
    }

    let layout = Layout::new(ctx, theme, screen, data, &rows, space);
    state.scroll = scroll_to(
        state.scroll,
        state.cursor.row,
        layout.visible,
        layout.list_len,
    );

    widgets::dialog_box(
        ctx,
        theme,
        Id::new(("menu", screen.title())),
        Order::Foreground,
        true,
        layout.rect,
        1.0,
        |ui, inner| {
            Painting {
                ui,
                theme,
                state,
                data,
                commands,
                screen,
                inner,
                // Only a pointer that moves may take the cursor, so a resting
                // mouse doesn't fight the keyboard.
                pointer_moved: ctx.input(|i| i.pointer.delta() != Vec2::ZERO),
            }
            .paint(&rows, &layout);
        },
    );
}

/// The dialog for typing a new save state's name.
fn draw_name_field(
    ctx: &egui::Context,
    theme: &Theme,
    state: &mut MenuState,
    data: &MenuData,
    name: &str,
    space: Rect,
    commands: &mut Vec<MenuCommand>,
) {
    // Wide enough for the longest name plus the text cursor.
    let widest = widgets::text_width(ctx, theme, &"M".repeat(MAX_STATE_NAME));
    let width = theme.snap(widest) + theme.px(CURSOR_WIDTH + 2.0 * DIALOG_PADDING);
    let size = Vec2::new(
        width.min(space.width()),
        theme.px(4.0 * ROW_HEIGHT + 2.0 * DIALOG_PADDING),
    );
    let offset = (space.size() - size) / 2.0;
    let min = space.min + Vec2::new(theme.snap(offset.x.max(0.0)), theme.snap(offset.y.max(0.0)));

    widgets::dialog_box(
        ctx,
        theme,
        Id::new("menu-name"),
        Order::Foreground,
        true,
        Rect::from_min_size(min, size),
        1.0,
        |ui, inner| {
            let row_height = theme.px(ROW_HEIGHT);
            let line = |row: f32| inner.min + Vec2::new(0.0, row_height * row);
            widgets::text(ui.painter(), theme, line(0.0), inner.width(), "New Save");
            widgets::text(ui.painter(), theme, line(1.5), inner.width(), name);
            let typed = theme.snap(widgets::text_width(ctx, theme, name));
            let caret = line(1.5) + Vec2::new(typed + theme.px(1.0), 0.0);
            widgets::text_caret(ui.painter(), theme, caret);

            let half = theme.snap(inner.width() / 2.0);
            let buttons = [("Save", Entry::CommitName), ("Cancel", Entry::CancelName)];
            for (col, (text, entry)) in buttons.into_iter().enumerate() {
                let rect = Rect::from_min_size(
                    line(3.0) + Vec2::new(half * col as f32, 0.0),
                    Vec2::new(half, row_height),
                );
                let id = Id::new(("menu-name-button", col));
                if widgets::menu_item(ui, theme, id, rect, text, false).clicked() {
                    state.activate(entry, data, commands);
                }
            }
        },
    );
}

fn scroll_with_wheel(
    ctx: &egui::Context,
    state: &mut MenuState,
    rows: &[Vec<Entry>],
    data: &MenuData,
    commands: &mut Vec<MenuCommand>,
) {
    let id = Id::new("menu-wheel");
    let delta = ctx.input(|i| i.raw_scroll_delta.y);
    if delta == 0.0 || state.is_capturing() {
        return;
    }
    let total = ctx.data(|d| d.get_temp::<f32>(id)).unwrap_or(0.0) + delta;
    let steps = (total / SCROLL_STEP).trunc();
    ctx.data_mut(|d| d.insert_temp(id, total - steps * SCROLL_STEP));
    // Wheel up is positive. Unlike the directional keys, the wheel doesn't wrap.
    let row = (state.cursor.row as isize - steps as isize)
        .clamp(0, rows.len().saturating_sub(1) as isize) as usize;
    state.cursor = Cursor {
        row,
        ..state.cursor
    };
    state.process_inputs(data, commands);
}

/// Where a screen's dialog goes and how much of its list fits.
struct Layout {
    rect: Rect,
    /// Number of entries in the scrolling list (the rest are footer rows).
    list_len: usize,
    /// How many list entries are shown at once.
    visible: usize,
    /// Lines of text each list entry takes.
    entry_lines: usize,
}

impl Layout {
    fn new(
        ctx: &egui::Context,
        theme: &Theme,
        screen: Screen,
        data: &MenuData,
        rows: &[Vec<Entry>],
        space: Rect,
    ) -> Layout {
        let width_of = |s: &str| widgets::text_width(ctx, theme, s);
        let list_len = match screen {
            Screen::SelectRom => data.roms.len(),
            Screen::InputBindings => data.bindings.len(),
            Screen::SaveState => data.states.len() + 1,
            Screen::LoadState => data.states.len(),
            Screen::Root | Screen::Options => rows.len(),
        };
        // A binding is its action's name with the two cells on the line below.
        let entry_lines = if screen == Screen::InputBindings {
            2
        } else {
            1
        };

        let item_width = |label: &str| theme.px(CURSOR_WIDTH) + width_of(label);
        let content_width = match screen {
            Screen::InputBindings => theme.px(BINDINGS_WIDTH),
            _ => {
                let empty_notice = (list_len == 0).then(|| width_of(empty_notice(screen)));
                rows.iter()
                    .flatten()
                    .map(|&entry| item_width(&label(entry, data)))
                    .chain(empty_notice)
                    .chain([width_of(screen.title()), theme.px(96.0)])
                    .fold(0.0, f32::max)
            }
        };

        // Title and its gap, plus (for list screens) the footer and arrow gaps,
        // or the empty-list notice.
        let footer_rows = rows.len().saturating_sub(list_len);
        let notice_rows = usize::from(list_len == 0);
        let chrome =
            theme.px((2 + footer_rows + notice_rows) as f32 * ROW_HEIGHT + 2.0 * DIALOG_PADDING);
        let entry_height = theme.px(entry_lines as f32 * ROW_HEIGHT);
        let fits = ((space.height() - chrome) / entry_height).floor() as usize;
        let visible = fits.max(1).min(list_len);

        let size = Vec2::new(
            (theme.snap(content_width) + theme.px(1.0 + 2.0 * DIALOG_PADDING)).min(space.width()),
            chrome + entry_height * visible as f32,
        );
        let offset = (space.size() - size) / 2.0;
        let min =
            space.min + Vec2::new(theme.snap(offset.x.max(0.0)), theme.snap(offset.y.max(0.0)));
        Layout {
            rect: Rect::from_min_size(min, size),
            list_len,
            visible,
            entry_lines,
        }
    }
}

fn label(entry: Entry, data: &MenuData) -> String {
    match entry {
        Entry::Open(Screen::SelectRom) if data.rom_loaded => "Change ROM".to_owned(),
        Entry::Open(screen) => screen.title().to_owned(),
        Entry::Exit => "Exit".to_owned(),
        Entry::Rom(index) => data.roms[index].label.clone(),
        Entry::NewState => "New Save".to_owned(),
        Entry::State(index) => data.states[index].label.clone(),
        Entry::CommitName => "Save".to_owned(),
        Entry::CancelName => "Cancel".to_owned(),
        Entry::Reset => "Reset Game".to_owned(),
        Entry::Binding { row, slot } => data.bindings[row]
            .triggers
            .get(slot)
            .map_or_else(|| "---".to_owned(), trigger_label),
        Entry::ResetBindings => "Reset All".to_owned(),
        Entry::Back => "Back".to_owned(),
    }
}

/// What a list screen says when it has nothing to list.
fn empty_notice(screen: Screen) -> &'static str {
    match screen {
        Screen::LoadState => NO_STATES,
        _ => NO_ROMS,
    }
}

/// Everything needed while painting the inside of a menu dialog.
struct Painting<'a, 'd> {
    ui: &'a mut egui::Ui,
    theme: &'a Theme,
    state: &'a mut MenuState,
    data: &'a MenuData<'d>,
    commands: &'a mut Vec<MenuCommand>,
    /// The screen being painted; a click can move `state` past it mid-frame.
    screen: Screen,
    inner: Rect,
    pointer_moved: bool,
}

impl Painting<'_, '_> {
    fn paint(&mut self, rows: &[Vec<Entry>], layout: &Layout) {
        let row_height = self.theme.px(ROW_HEIGHT);
        let top = self.inner.min.y;
        widgets::text(
            self.ui.painter(),
            self.theme,
            Pos2::new(self.inner.min.x, top),
            self.inner.width(),
            self.screen.title(),
        );

        // The half-row gaps above and below the list hold the scroll arrows.
        let list_top = top + row_height * 1.5;
        let first = self.state.scroll;
        let shown = first..first + layout.visible;
        let entry_height = row_height * layout.entry_lines as f32;
        for (index, row) in shown.clone().enumerate() {
            self.row(rows, row, list_top + entry_height * index as f32, layout);
        }

        if layout.list_len == 0 {
            widgets::text(
                self.ui.painter(),
                self.theme,
                Pos2::new(self.inner.min.x, list_top),
                self.inner.width(),
                empty_notice(self.screen),
            );
        }

        let notice_rows = usize::from(layout.list_len == 0);
        let list_bottom =
            list_top + entry_height * layout.visible as f32 + row_height * notice_rows as f32;
        let arrow_x = self.inner.max.x - self.theme.px(7.0);
        if first > 0 {
            let pos = Pos2::new(arrow_x, list_top - self.theme.px(5.0));
            widgets::scroll_arrow(self.ui.painter(), self.theme, pos, Arrow::Up);
        }
        if shown.end < layout.list_len {
            let pos = Pos2::new(arrow_x, list_bottom + self.theme.px(1.0));
            widgets::scroll_arrow(self.ui.painter(), self.theme, pos, Arrow::Down);
        }

        for (line, row) in (layout.list_len..rows.len()).enumerate() {
            let y = list_bottom + row_height * (line as f32 + 0.5);
            self.row(rows, row, y, layout);
        }
    }

    fn row(&mut self, rows: &[Vec<Entry>], row: usize, y: f32, layout: &Layout) {
        let Some(columns) = rows.get(row) else {
            return;
        };
        let height = self.theme.px(ROW_HEIGHT);
        let left = self.inner.min.x;

        if self.screen == Screen::InputBindings && row < layout.list_len {
            widgets::text(
                self.ui.painter(),
                self.theme,
                Pos2::new(left, y),
                self.inner.width(),
                &self.data.bindings[row].display,
            );
            let stride = self.theme.snap(self.inner.width() / BINDING_SLOTS as f32);
            for (col, &entry) in columns.iter().enumerate() {
                let x = left + stride * col as f32;
                let size = Vec2::new(stride - self.theme.px(CLEAR_WIDTH), height);
                let rect = Rect::from_min_size(Pos2::new(x, y + height), size);
                self.binding_cell(Cursor { row, col }, entry, rect);
            }
        } else {
            let width = self.inner.width() / columns.len() as f32;
            for (col, &entry) in columns.iter().enumerate() {
                let x = left + self.theme.snap(width * col as f32);
                let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height));
                let text = label(entry, self.data);
                self.item(Cursor { row, col }, entry, rect, &text);
            }
        }
    }

    fn binding_cell(&mut self, at: Cursor, entry: Entry, rect: Rect) {
        let Entry::Binding { row, slot } = entry else {
            return;
        };
        let binding = &self.data.bindings[row];
        let capturing_here = self.state.is_capturing_cell(binding, slot);
        let text = if capturing_here {
            "press..".to_owned()
        } else {
            label(entry, self.data)
        };
        self.item(at, entry, rect, &text);

        // Mouse shortcuts after the cell: cancel a pending rebind (every key is
        // bindable, so none can mean "cancel"), or unbind a filled cell.
        let beside = Rect::from_min_size(
            rect.right_top(),
            Vec2::new(self.theme.px(CLEAR_WIDTH), rect.height()),
        );
        if capturing_here || binding.triggers.get(slot).is_some() {
            let id = Id::new(("menu-clear", row, slot));
            let response = widgets::menu_item(self.ui, self.theme, id, beside, "x", false);
            if response.clicked() {
                if capturing_here {
                    self.state.cancel_capture();
                } else {
                    clear_binding(binding, slot, self.commands);
                }
            }
        }
    }

    fn item(&mut self, at: Cursor, entry: Entry, rect: Rect, text: &str) {
        let selected = self.state.cursor == at;
        let id = Id::new(("menu-item", at.row, at.col));
        let response = widgets::menu_item(self.ui, self.theme, id, rect, text, selected);
        if self.state.is_capturing()
            || self.state.is_naming()
            || self.state.screen() != Some(self.screen)
        {
            return;
        }
        if response.clicked() {
            self.state.cursor = at;
            self.state.activate(entry, self.data, self.commands);
        } else if self.pointer_moved && response.hovered() {
            self.state.cursor = at;
        }
    }
}
