use crate::resource::{Amount, RESOURCE_COUNT};

pub const HEX_SIZE: f64 = 32.0;
pub const MAX_FRAME_DT: f64 = 0.1;

pub const MIN_ZOOM: f64 = 0.2;
pub const MAX_ZOOM: f64 = 8.0;

pub const TOOLTIP_OFFSET: f64 = 14.0;
pub const ICON_AREA: f64 = 0.6;

pub const COUNT_WEIGHTS: [f64; RESOURCE_COUNT] = [85.0, 10.0, 4.0, 0.9, 0.1];

pub const INITIAL_MAX_GENERATE: Amount = 10;
pub const INITIAL_MAX_OVERFILL: Amount = 90;
pub const INITIAL_PER_CLICK: Amount = 1;
