use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    F32,
    F64,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    F32(f32),
    F64(f64),
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
}

impl ParamValue {
    pub fn as_f32(&self) -> f32 {
        match *self {
            Self::F32(v) => v,
            Self::F64(v) => v as f32,
            Self::U8(v) => v as f32,
            Self::I8(v) => v as f32,
            Self::U16(v) => v as f32,
            Self::I16(v) => v as f32,
            Self::U32(v) => v as f32,
            Self::I32(v) => v as f32,
            Self::U64(v) => v as f32,
            Self::I64(v) => v as f32,
        }
    }

    pub fn kind(&self) -> ParamKind {
        match self {
            Self::F32(_) => ParamKind::F32,
            Self::F64(_) => ParamKind::F64,
            Self::U8(_) => ParamKind::U8,
            Self::I8(_) => ParamKind::I8,
            Self::U16(_) => ParamKind::U16,
            Self::I16(_) => ParamKind::I16,
            Self::U32(_) => ParamKind::U32,
            Self::I32(_) => ParamKind::I32,
            Self::U64(_) => ParamKind::U64,
            Self::I64(_) => ParamKind::I64,
        }
    }
}

impl fmt::Display for ParamValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::F32(v) => write!(f, "{v:.4}"),
            Self::F64(v) => write!(f, "{v:.4}"),
            Self::U8(v) => write!(f, "{v}"),
            Self::I8(v) => write!(f, "{v}"),
            Self::U16(v) => write!(f, "{v}"),
            Self::I16(v) => write!(f, "{v}"),
            Self::U32(v) => write!(f, "{v}"),
            Self::I32(v) => write!(f, "{v}"),
            Self::U64(v) => write!(f, "{v}"),
            Self::I64(v) => write!(f, "{v}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamRange {
    F32 { min: f32, max: f32 },
    F64 { min: f64, max: f64 },
    U8 { min: u8, max: u8 },
    I8 { min: i8, max: i8 },
    U16 { min: u16, max: u16 },
    I16 { min: i16, max: i16 },
    U32 { min: u32, max: u32 },
    I32 { min: i32, max: i32 },
    U64 { min: u64, max: u64 },
    I64 { min: i64, max: i64 },
}

impl ParamRange {
    pub fn full(kind: ParamKind) -> Self {
        match kind {
            ParamKind::F32 => Self::F32 { min: f32::MIN, max: f32::MAX },
            ParamKind::F64 => Self::F64 { min: f64::MIN, max: f64::MAX },
            ParamKind::U8 => Self::U8 { min: u8::MIN, max: u8::MAX },
            ParamKind::I8 => Self::I8 { min: i8::MIN, max: i8::MAX },
            ParamKind::U16 => Self::U16 { min: u16::MIN, max: u16::MAX },
            ParamKind::I16 => Self::I16 { min: i16::MIN, max: i16::MAX },
            ParamKind::U32 => Self::U32 { min: u32::MIN, max: u32::MAX },
            ParamKind::I32 => Self::I32 { min: i32::MIN, max: i32::MAX },
            ParamKind::U64 => Self::U64 { min: u64::MIN, max: u64::MAX },
            ParamKind::I64 => Self::I64 { min: i64::MIN, max: i64::MAX },
        }
    }

    pub fn kind(&self) -> ParamKind {
        match self {
            Self::F32 { .. } => ParamKind::F32,
            Self::F64 { .. } => ParamKind::F64,
            Self::U8 { .. } => ParamKind::U8,
            Self::I8 { .. } => ParamKind::I8,
            Self::U16 { .. } => ParamKind::U16,
            Self::I16 { .. } => ParamKind::I16,
            Self::U32 { .. } => ParamKind::U32,
            Self::I32 { .. } => ParamKind::I32,
            Self::U64 { .. } => ParamKind::U64,
            Self::I64 { .. } => ParamKind::I64,
        }
    }

    pub fn step_count(&self) -> Option<usize> {
        match *self {
            Self::F32 { .. } | Self::F64 { .. } => None,
            Self::U8 { min, max } => Some((max - min) as usize + 1),
            Self::I8 { min, max } => Some((max as i16 - min as i16) as usize + 1),
            Self::U16 { min, max } => Some((max - min) as usize + 1),
            Self::I16 { min, max } => Some((max as i32 - min as i32) as usize + 1),
            Self::U32 { min, max } => Some((max - min) as usize + 1),
            Self::I32 { min, max } => Some((max as i64 - min as i64) as usize + 1),
            Self::U64 { min, max } => Some((max - min) as usize + 1),
            Self::I64 { min, max } => Some((max as i128 - min as i128) as usize + 1),
        }
    }

    pub fn lerp(&self, t: f32) -> ParamValue {
        let t = t.clamp(0.0, 1.0);
        match *self {
            Self::F32 { min, max } => ParamValue::F32(min + t * (max - min)),
            Self::F64 { min, max } => ParamValue::F64(min + t as f64 * ((max - min))),
            Self::U8 { min, max } => {
                ParamValue::U8(min + (t * (max - min) as f32).round() as u8)
            }
            Self::I8 { min, max } => {
                ParamValue::I8(min + (t * (max - min) as f32).round() as i8)
            }
            Self::U16 { min, max } => {
                ParamValue::U16(min + (t * (max - min) as f32).round() as u16)
            }
            Self::I16 { min, max } => {
                ParamValue::I16(min + (t * (max - min) as f32).round() as i16)
            }
            Self::U32 { min, max } => {
                ParamValue::U32(min + (t * (max - min) as f32).round() as u32)
            }
            Self::I32 { min, max } => {
                ParamValue::I32(min + (t * (max - min) as f32).round() as i32)
            }
            Self::U64 { min, max } => {
                ParamValue::U64(min + (t * (max - min) as f32).round() as u64)
            }
            Self::I64 { min, max } => {
                ParamValue::I64(min + (t * (max - min) as f32).round() as i64)
            }
        }
    }
}

pub struct ParamDef {
    pub description: String,
    pub range: ParamRange,
    pub formatter: Option<Box<dyn Fn(&ParamValue) -> String>>,
}

impl ParamDef {
    pub fn format_value(&self, value: &ParamValue) -> String {
        match &self.formatter {
            Some(f) => f(value),
            None => value.to_string(),
        }
    }
}
