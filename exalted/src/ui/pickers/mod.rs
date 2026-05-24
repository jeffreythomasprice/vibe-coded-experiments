//! Modal pickers for selecting charms / spells / backgrounds out of the
//! embedded rules database. Each picker's `show` function drives one frame
//! and returns a `PickerOutcome` so the calling section can decide whether
//! to close it.

pub mod background_picker;
pub mod charm_picker;
pub mod spell_picker;

pub enum PickerOutcome<T> {
    Stay,
    Picked(T),
    Cancelled,
}
