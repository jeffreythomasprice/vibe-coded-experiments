//! Defined in `shared/schemas/battle.json`; see `shared/schemas/README.md`. Deliberately kept
//! externally tagged on the wire (unlike `protocol`'s envelopes) since `BattleEvent` is persisted
//! in the browser's `localStorage` and in the DynamoDB rooms table via `BattleLog`.

pub use crate::generated::{BattleEvent, InterruptReason};
