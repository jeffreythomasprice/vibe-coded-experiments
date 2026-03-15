#[allow(dead_code)]
mod inner {
    include!(concat!(env!("OUT_DIR"), "/schema_types.rs"));
}

pub use inner::*;
