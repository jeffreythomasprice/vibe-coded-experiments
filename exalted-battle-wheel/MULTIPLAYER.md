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

## Invite links

The room panel shows a **Copy invite link** button to whoever is hosting. It builds a URL of the
form `?auth_code=<code>&join_room=<room>` for the client's own domain and copies it to the
clipboard (or, if the clipboard is unavailable — an insecure origin, or the browser refuses it —
reveals it in a field to copy by hand instead).

Opening that link on startup: this browser's own access code is checked first, and kept if it
still works — the link's code is only adopted when this browser has no working code of its own,
so an invite link can never silently replace one. If the link names the room this browser was
already remembering (see "Remembering a room" below), it resumes that session instead of joining
fresh, so a host reopening their own link keeps their host status rather than rejoining as an
ordinary member. Both parameters are stripped from the address bar immediately after they're
read, so neither the code nor the room name lingers in browser history.

Treat an invite link as a bearer credential, not just a room pointer: whoever has it can sign in
as that access code until it's revoked, for anything the app lets that code do — not only this
room. The code it carries is deliberately never an admin's own: an admin's invite link hands out
the newest non-admin code the server knows about instead (an error if there isn't one), so an
invite can never hand out the right to manage every other code.

## Administering rooms

Whoever holds an admin access code (`README.md`'s `is_admin` — admin *access-code* status, unrelated
to write access inside any one room, same as the opening paragraph above says) sees an **All rooms**
button in the Multiplayer panel, both from Solo and from inside a room of their own. It opens every
room currently live on the server: searchable by name, paged, each row with its own **Copy link**
(the same invite link a host would build, never an admin's own code) and a **Close room** that
deletes the room outright after a confirmation. Everyone in a closed room is disconnected
immediately, drops to Solo keeping their own local copy of whatever battle the room had, and sees a
message that an administrator closed their room. A room this list doesn't show yet just hasn't been
searched for or paged to — the list itself is a plain `GET /rooms`, admin-only (see `README.md`), so
nothing here is a special path around the room store.

## Connecting

1. **Multiplayer (Solo)** → name yourself → type a room name → **Host a room**, or pick **Join a
   room** and use a name someone else already hosted under. A name isn't required up front — one
   is suggested automatically the first time it's needed.
2. Joining adopts whatever battle the room currently has; hosting seeds the room with your own.
3. From the room panel, rename yourself at any time, and — if you have write access — grant or
   revoke anyone else's, or kick them.

## Manual test: two tabs, one machine

1. `./dev.sh`, then open `http://127.0.0.1:8000/` in two tabs.
2. Sign each tab in with one of the two access codes `./dev.sh` printed, so the tabs are genuinely
   distinct connections.
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
9. From A (the host), **Copy invite link**, then open it in a private window: confirm it signs in,
   joins the room, the address bar ends up clean, and the guest appears in A's roster under a
   suggested name. Reopen A's own invite link from A itself: confirm A stays host rather than
   rejoining as an ordinary member.
10. From whichever of A/B is signed in with the admin code, open **All rooms**; confirm the room
    from step 3 is listed with its current member count, that typing part of its name narrows the
    list to it, and that clearing the search restores the rest. **Close** it and confirm the
    dialog; the other tab should drop to Solo keeping its own local copy of the battle and show
    "This room was closed by an administrator." Confirm the closed room no longer appears in
    **All rooms** (refresh the search if needed) and that `curl -H "Authorization: Bearer
    $MEMBER_CODE" localhost:8001/rooms` gets `403`.

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
6. From the host's side, **Copy invite link** and send it to the other computer by whatever means
   is actually at hand (chat, email); confirm opening it there joins the room over the real
   network. On a browser without HTTPS or `localhost` (a plain LAN IP over http, say), confirm the
   clipboard write is skipped silently and the revealed field still has the working link.
