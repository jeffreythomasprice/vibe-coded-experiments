# Multiplayer

Rooms are hosted by the server: every browser in a room holds a websocket connection to it, the
server owns the shared battle log, and it checks every move for legality and for write access
before applying it and telling everyone the result. There's nothing for two browsers to disagree
about — a browser's own state is never more than whatever the server's last message said it is.

Any access code can create a room, join a room, and play — access-code admin status (who may
manage access codes) is unrelated to what you can do inside a room. Inside a room, write access is
per connection:

- **Write access** lets you add and remove combatants, declare actions, advance the tick, undo,
  and grant or revoke anyone else's write access — except your own and the host's.
- **Read-only** members watch every change as it happens, and may still rename themselves, leave,
  and ask the server to resend the current state. Everything else is refused.
- The **host** is whoever created the room. The host always has write access and can never be
  demoted or kicked, by anyone, including themselves. If the host's connection drops, the room
  keeps running for everyone else; nobody else ever becomes host in their place.
- **Everyone who joins can edit** is a room-level setting, chosen when the room is created and
  changeable later by anyone with write access. It only sets what a *future* joiner starts as —
  never retroactive to anyone already in the room.

A room outlives its members: the last person leaving doesn't delete it, so its battle is still
there if someone rejoins later. Idle rooms and connections expire after 30 minutes of inactivity.

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

## Manual test: two computers, two networks

The real test — confirms multiplayer works over the internet, not just on one machine.

1. Deploy the current code (`./deploy.sh` — run this yourself; see `CLAUDE.md`) and use
   `https://exalted.jeffrey.lol` on both computers.
2. Follow "Connecting" above between the two computers, each with its own access code.
3. Exercise the real mechanics: add a combatant from each side, advance the tick, undo — confirm
   changes land on both screens.
4. Turn off wifi on one side briefly and back on; confirm it reconnects to the room on its own
   within a few seconds and picks the current battle back up.
