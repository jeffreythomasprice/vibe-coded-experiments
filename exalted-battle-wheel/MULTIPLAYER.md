# Multiplayer

Rooms are hosted by the server: every browser in a room holds a websocket connection to it, the
server owns the shared battle log, and it checks every move for legality and for write access
before applying it and telling everyone the result. There's nothing for two browsers to disagree
about — a browser's own state is never more than whatever the server's last message said it is.

The wire protocol itself — every message type below, and every REST request/response — is defined
by JSON Schema in `shared/schemas/`, generated into Rust by `shared/build.rs`, and validated
against that same schema on every decode, both directions (see `shared/src/validate.rs`). A message
that doesn't match its schema — an unknown `type`, a name over the length bound, the wrong shape
entirely — is rejected before it's ever deserialized, not just whatever `serde` happens to do with
it. `ClientMessage`/`ServerMessage`/`BattleRequest`/`BattleCommand`/`ProtocolError` are adjacently
tagged (`{"type":"Join","data":{...}}`); this is a breaking wire change for a client mid-connection
across a deploy, though rooms' 30-minute idle TTL and the fact that deploys are manual keep the
practical blast radius to "reconnects." `BattleEvent` and everything it carries keep serde's older
externally-tagged encoding instead, deliberately: `BattleLog` (which embeds a `Vec<BattleEvent>`) is
persisted in the browser's own `localStorage` for Solo mode, and retagging it would have silently
broken every already-saved battle.

Any access code can create a room, join a room, and play — access-code admin status (who may
manage access codes) is unrelated to what you can do inside a room. Inside a room, write access is
per connection:

- **Write access** lets you add and remove combatants, declare actions, advance the tick, undo,
  and grant or revoke anyone else's write access — except your own and the host's.
- **Read-only** members watch every change as it happens, and may still rename themselves, leave,
  and ask the server to resend the current state. Everything else is refused.
- The **host** is whoever created the room. The host always has write access and can never be
  demoted or kicked, by anyone, including themselves. If the host's connection drops, the room
  keeps running for everyone else, and nobody else becomes host in their place — but the browser
  that was hosting reclaims it automatically if it comes back (see "Remembering a room" below).
  Two connections can hold host at once if that happens while the original is still around.
- **Everyone who joins can edit** is a room-level setting, chosen when the room is created and
  changeable later by anyone with write access. It only sets what a *future* joiner starts as —
  never retroactive to anyone already in the room.

A room outlives its members: the last person leaving doesn't delete it, so its battle is still
there if someone rejoins later. Idle rooms and connections expire after 30 minutes of inactivity.

## Remembering a room

Every member is handed a signed, expiring session token on join, which the browser keeps in local
storage and presents again on its very next page load — closing the tab (or the whole browser) and
reopening it lands back in the same room automatically, host status included, without retyping
anything. A token that no longer checks out (the room's gone, the token expired, or it names a
room that got reaped and its name reused by someone else's) fails silently into Solo instead of
getting stuck: whatever went wrong is toasted once, and the stale token is cleared.

## Connecting

1. **Multiplayer (Solo)** → name yourself → type a room name → **Host a room**, or pick **Join a
   room** and use a name someone else already hosted under.
2. Joining adopts whatever battle the room currently has; hosting seeds the room with your own.
3. From the room panel, rename yourself at any time, and — if you have write access — grant or
   revoke anyone else's, or kick them.

## Manual test: two tabs, one machine

1. `./dev.sh`, then open `http://127.0.0.1:8000/` in two tabs.
2. Sign both in with an access code (`local-admin`, or create a second code from the settings
   dialog so the two tabs are genuinely distinct connections).
3. Add a combatant in tab A, then **Host a room** under some name. In tab B, **Join a room** under
   that same name and confirm it adopts A's battle.
4. Advance the tick in either tab and confirm both follow.
5. From A, take away B's write access; confirm B's editing controls grey out immediately and a
   move it tries anyway is refused. Grant it back.
6. Confirm B cannot touch A's (the host's) write access, and that renaming works from B while
   read-only.
7. Kick B from A; confirm B drops to Solo keeping its own local copy of the battle, then rejoins
   under the same room name.
8. Hard-close tab A (not just Leave) and reopen `http://127.0.0.1:8000/`. Confirm it lands straight
   back in the room with the current battle and its own Host badge, and that B's roster shows A
   back and can be administered by it again.

## Manual test: two computers, two networks

The real test — confirms multiplayer works over the internet, not just on one machine.

1. Deploy the current code (`./deploy.sh` — run this yourself; see `CLAUDE.md`) and use
   `https://exalted.jeffrey.lol` on both computers.
2. Follow "Connecting" above between the two computers, each with its own access code.
3. Exercise the real mechanics: add a combatant from each side, advance the tick, undo — confirm
   changes land on both screens.
4. Turn off wifi on one side briefly and back on; confirm it reconnects to the room on its own
   within a few seconds and picks the current battle back up.
5. Close the browser entirely on one side and reopen it later; confirm it rejoins the room on its
   own, host status included if it was hosting.
