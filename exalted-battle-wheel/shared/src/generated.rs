//! Every type `shared/build.rs` compiles from `shared/schemas/*.json`. Never edited directly --
//! see `shared/schemas/README.md` for how to change a wire type.

include!(concat!(env!("OUT_DIR"), "/wire.rs"));
