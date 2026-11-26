//! Cursor movement over a menu, independent of egui and of any input device.

/// An abstract menu control. The host translates its keyboard/gamepad input into
/// these and queues them with [`crate::MenuState::navigate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuInput {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    /// Unbind the binding under the cursor.
    Clear,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cursor {
    pub row: usize,
    pub col: usize,
}

impl Cursor {
    /// Pull the cursor back inside a menu of `rows` rows whose row `r` has
    /// `cols(r)` entries.
    pub fn clamped(self, rows: usize, cols: impl Fn(usize) -> usize) -> Cursor {
        let row = self.row.min(rows.saturating_sub(1));
        let col = self.col.min(cols(row).saturating_sub(1));
        Cursor { row, col }
    }

    /// Apply a directional input. Rows wrap around; columns stop at the ends.
    pub fn moved(self, input: MenuInput, rows: usize, cols: impl Fn(usize) -> usize) -> Cursor {
        if rows == 0 {
            return Cursor::default();
        }
        let mut next = self.clamped(rows, &cols);
        match input {
            MenuInput::Up => next.row = (next.row + rows - 1) % rows,
            MenuInput::Down => next.row = (next.row + 1) % rows,
            MenuInput::Left => next.col = next.col.saturating_sub(1),
            MenuInput::Right => next.col += 1,
            MenuInput::Confirm | MenuInput::Back | MenuInput::Clear => {}
        }
        next.clamped(rows, cols)
    }
}

/// The first visible entry of a list showing `visible` of `len` entries: moved
/// from `top` only as far as needed to keep `selected` on screen. A `selected`
/// past the end (a footer row) leaves the list where it is.
pub(crate) fn scroll_to(top: usize, selected: usize, visible: usize, len: usize) -> usize {
    let top = if selected >= len {
        top
    } else if selected < top {
        selected
    } else if selected >= top + visible {
        selected + 1 - visible
    } else {
        top
    };
    top.min(len.saturating_sub(visible))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(row: usize, col: usize) -> Cursor {
        Cursor { row, col }
    }

    #[test]
    fn rows_wrap_and_columns_stop_at_the_ends() {
        let cols = |_| 2;
        assert_eq!(at(0, 0).moved(MenuInput::Up, 3, cols), at(2, 0));
        assert_eq!(at(2, 0).moved(MenuInput::Down, 3, cols), at(0, 0));
        assert_eq!(at(0, 0).moved(MenuInput::Left, 3, cols), at(0, 0));
        assert_eq!(at(0, 1).moved(MenuInput::Right, 3, cols), at(0, 1));
    }

    #[test]
    fn moving_to_a_shorter_row_clamps_the_column() {
        let cols = |row| if row == 0 { 2 } else { 1 };
        assert_eq!(at(0, 1).moved(MenuInput::Down, 2, cols), at(1, 0));
    }

    #[test]
    fn a_stale_cursor_is_clamped() {
        assert_eq!(at(9, 9).clamped(3, |_| 1), at(2, 0));
        assert_eq!(at(9, 9).clamped(0, |_| 0), at(0, 0));
    }

    #[test]
    fn scrolling_follows_the_selection_only_when_needed() {
        assert_eq!(scroll_to(0, 2, 3, 10), 0);
        assert_eq!(scroll_to(0, 3, 3, 10), 1);
        assert_eq!(scroll_to(5, 1, 3, 10), 1);
        // A footer row doesn't move the list.
        assert_eq!(scroll_to(4, 10, 3, 10), 4);
        // Never scroll past the end, e.g. after the window grows.
        assert_eq!(scroll_to(7, 9, 8, 10), 2);
    }
}
