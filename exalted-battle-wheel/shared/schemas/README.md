# Wire schemas

These five files are the source of truth for every DTO that crosses the network: every REST
request/response body (`access.json`, `rooms.json`) and every websocket message
(`ws.json`, `battle.json`). `shared/build.rs` merges their `$defs` into one document and feeds it
to [`typify`](https://github.com/oxidecomputer/typify), which generates the Rust types consumed
by both `server` and `client` — see each generated type's re-export site in `shared/src/` for where
it lands.

## Authoring rules

- Each file is a bare `{"$defs": {...}}` map. `build.rs` errors on a name defined in more than one
  file.
- `$ref` a def from any file as `#/$defs/Name` — the merge happens before typify ever sees the
  document, so cross-file refs resolve exactly like same-file ones.
- No `title` keys. typify uses a def's own key (the map key under `$defs`) to name the Rust type
  it generates; a stray `title` inside the schema body is at best redundant and at worst
  ambiguous, so we leave it out everywhere.
- These are schemars-0.8-flavoured schemas (draft-07-ish, `definitions`/`$defs` interchangeable,
  the same enum/newtype/`additionalProperties` conventions schemars' derive produces) because
  that's exactly what typify 0.8 is tested against. If you're extending a def by hand, the fastest
  way to get the shape right is to look at what schemars would produce for the equivalent Rust
  type, not to guess at bare JSON Schema.

## Tagging

Five enums — `ClientMessage`, `ServerMessage`, `BattleRequest`, `BattleCommand`, `ProtocolError` —
use adjacent tagging (`{"type": "Join", "data": {...}}`), which shows up in the schema as each
`oneOf` branch requiring a `type` const alongside an optional `data`.

Everything under `battle.json` (`BattleEvent` and everything it carries) is deliberately **not**
retagged and keeps serde's externally-tagged default (`{"AddCombatant": {...}}`, or a bare string
for a unit variant). `BattleLog` — which contains a `Vec<BattleEvent>` — is persisted in the
browser's `localStorage` (`ebw.battle`) and in the DynamoDB rooms table, so changing this encoding
would silently break every already-saved battle. If you're tempted to make `battle.json` consistent
with `ws.json`, don't, unless you're also shipping a migration for existing saved state.

## Types the schema describes but doesn't generate

`common.json` holds a few defs that exist so the document stays a complete wire spec, but that
`shared/build.rs` tells typify to map onto a hand-written Rust type instead of generating
(`TypeSpaceSettings::with_replacement`):

| Def | Rust type | Why |
|---|---|---|
| `BattleLog` | `crate::battle::BattleLog` | Private fields plus a validating `#[serde(try_from = "RestoredLog")]` deserializer that typify can't express. |
| `Timestamp` | `crate::timestamp::Timestamp` | Needs a field-level `#[serde(with = "time::serde::rfc3339")]`-equivalent, which generated code can't carry. |
| `SessionRejection` | `crate::protocol::SessionRejection` | Its `thiserror` messages are user-facing and hand-written; typify's auto `Display` would collide with them. |
| `Index` | `usize` | JSON Schema's `"format": "uint"` is the only one typify maps to an unsized-width Rust type, and it always picks `u32` for it -- a def that genuinely needs `usize` (a `Vec` index or cursor) has no other way to say so. |

If you add a new def that needs the same treatment, add it here and to the `REPLACEMENTS` list in
`build.rs`.

## Constrained scalars, and what enforces them

`common.json` also defines a handful of bounded string scalars -- `MemberName`, `RoomName`,
`AccessKey`, `Label`, `Note` -- reused by `$ref` wherever the corresponding kind of text appears.
Unlike the replaced types above, these *are* generated: a named string def with `minLength`/
`maxLength` gets typify's constrained-newtype treatment (a private-field wrapper with a hand-written
validating `Deserialize`, `FromStr`, and `TryFrom`, but no public constructor), so the bound is
enforced the moment one of these is deserialized, with no code in `server` or `client` needing to
remember to check it. This is layer one of two:

1. **Compiled into `Deserialize`** -- what's described above. Free, and always on, but only works
   for `minLength`/`maxLength`/`pattern`/enum-style constraints on a *named* def; an inline
   constraint on a struct property doesn't get this treatment, which is why e.g. `Member.name`
   `$ref`s `MemberName` rather than declaring `"maxLength": 40` on the property directly.
2. **`shared::validate::decode`** -- everything layer one can't express (cross-field rules, or just
   whatever a future schema needs that typify doesn't compile in), checked against the *exact* JSON
   Schema document these types were generated from before the JSON is ever deserialized. See
   `shared/src/validate.rs`'s own doc comment, and `server`/`client`'s `ws`/`net` modules for where
   each side's actual decode calls go through it.

Two of these bounds mirror constants that predate the schema and must not drift from it:
`MemberName`'s `maxLength` mirrors `protocol::name::MAX_NAME_LEN`, and `RoomName`'s mirrors
`MAX_ROOM_NAME_LEN` — a test in `shared/src/protocol/name.rs` checks both stay equal, since typify
inlines the literal bound into generated code with no way to export it as a constant to compare
against directly. `Label`'s and `Note`'s bounds (120 and 1000 characters) have their own mirrored
constants in `shared::battle` (`MAX_LABEL_LEN`, `MAX_NOTE_LEN`), used by the `label()`/`note()`
truncating helpers there rather than by a generated type's own comparison test.
