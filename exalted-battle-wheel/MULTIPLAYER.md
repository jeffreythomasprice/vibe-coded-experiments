# Multiplayer

Rooms connect browsers directly, peer-to-peer (WebRTC), with no server in between. There's no
rendezvous service either, so the two sides exchange connection info by hand: one side hosts and
shares an invite (a text code, or a QR code for screen-share/phone-camera convenience), the other
pastes or scans it back a reply the same way. Once connected, every change either side makes is
proposed to the room and only takes effect once everyone agrees — a genuine disagreement
disconnects everyone with an error rather than letting the battles quietly drift apart.

There's no TURN/relay server and no authentication:

- If both sides sit behind networks that can't reach each other directly (see "NAT hairpinning"
  below, and note some corporate networks and carrier-grade NAT setups block this outright), the
  connection will simply fail. You'll get an honest "could not connect" error rather than an
  indefinite hang — within about 15 seconds on the host's side once it accepts your reply, or up to
  a minute on the joining side, since that wait also covers the time it takes a human to carry the
  reply code back to the host (a QR hand-off to a phone included) rather than just the network.
- The "everyone who joins can change the battle" flag and the kick button are conveniences, not
  security. Nothing stops a modified client from ignoring either.

## Connecting

1. One side: **Multiplayer (Solo) → name yourself → Host a room**. A spinner runs while the invite
   is prepared; the code and QR appear together a few seconds later.
2. Send the invite to the other side — paste the text, or let them scan the QR. The QR encodes a
   full link; scanning it (or opening it directly) drops the other side straight into the join
   flow with the code already filled in.
3. Other side: name yourself, confirm **Join**. A spinner runs the same way, then a reply code/QR
   of its own appears.
4. Send the reply back to the host the same way, paste it into "Paste their reply code here", and
   click **Connect**.
5. Once the peer list shows both names, you're connected. Joining adopts whatever battle the host
   currently has; hosting never resets your own.

## Manual test: two tabs, one machine

The quick way to sanity-check the feature without involving a second device.

1. `trunk serve`, open `http://127.0.0.1:8000/` in two tabs.
2. Follow "Connecting" above between the two tabs. The joining tab's own clock on this starts the
   moment it produces its reply code, before you've pasted that back into the host — so don't leave
   the reply sitting copied while you read something else; paste it into the host and click
   **Connect** right away, the same as you would with a real second person.
3. Once the host clicks **Connect**, it should connect within a few seconds. If it connects, you're
   done.

**If it times out** ("Could not connect — this may be a restrictive network..."), see
"NAT hairpinning" below — it's expected on a lot of home routers when testing this way, not a bug.

## Manual test: two computers, two networks

The real test — confirms nothing here secretly needs a server or a shared network.

**Prerequisite:** the app needs to be reachable from both machines. Either:

- Deploy the current code (`./deploy.sh` — run this yourself; see `CLAUDE.md`) and use
  `https://exalted.jeffrey.lol` on both computers, or
- Keep it local and tunnel `trunk serve` (ngrok, Tailscale Funnel, Cloudflare Tunnel) to a
  temporary public URL.

**Use genuinely different networks** — e.g. a laptop on home WiFi plus a phone on cellular data
with WiFi off, or coordinate with someone at a different location. Two devices on the *same* WiFi
don't really exercise this; local traffic wouldn't need STUN in the first place.

1. Follow "Connecting" above between the two computers.
2. It should connect within a few seconds on typical home/cellular networks. A timeout here points
   at one side's network (no TURN is configured — see the top of this document) rather than a bug;
   swapping one side for a phone hotspot is a quick way to confirm that.
3. Once connected, exercise the real mechanics: add a combatant from each side, advance the tick,
   undo — confirm changes land on both screens. Kick from the host and confirm the kicked side
   sees "The host removed you from the room," drops back to Solo, and keeps its own local battle.

## NAT hairpinning (and the localhost workaround)

Two tabs on one machine each discover their own "public" address via STUN — connecting to that
address means asking your own router to send the traffic back inside to yourself
("hairpinning"). Plenty of consumer routers refuse to do this, so two tabs on one machine can fail
to connect to *each other* even though the app and the codes are working correctly. This is purely
a same-machine testing artifact; it doesn't affect two separate computers on separate networks.

To work around it, force the app off the STUN path and onto the local-network path instead. The
easiest way is the room modal's own **Advanced** section:

1. Open the **Multiplayer** modal → **Advanced** → remove every STUN server listed (or replace
   them with an address that will never respond — see below) in both tabs.
2. Host/join as usual. With no reachable STUN server, ICE gathering finds no "public" candidate and
   falls back to the local candidate Chrome generates on its own, so two tabs on one machine can
   always reach each other directly. The invite code will be noticeably longer than normal (it's
   carrying the full connection info instead of the compact form) — that's expected.

The same override is also available as a `#stun=` URL fragment, read once at page load — useful
for seeding a fresh tab (or a deep link) without having to click through Advanced by hand:

1. Open a **new tab** (or navigate from a different page first) — don't just edit the fragment on
   a tab already sitting on the app, since a fragment-only change on an already-loaded page won't
   re-trigger the startup logic that reads it.
2. Navigate to:
   ```
   http://127.0.0.1:8000/#stun=stun:198.51.100.1:3478
   ```
   That address is reserved for documentation (RFC 5737) and will never respond, giving the same
   effect as an empty list once ICE gathering waits out its ~3 second timeout.
3. Do the same in the second tab, then host/join as usual — it should connect within a couple
   seconds.

`#stun=` accepts a comma-separated list of STUN server URLs and seeds the same list the Advanced
section edits; entries that aren't valid `stun:`/`stuns:` URLs are rejected at boot with an error
toast instead of silently breaking the next connection. Neither the fragment nor an edit made in
Advanced is saved anywhere — the list lives only in that tab's memory, and reloading the page
always restores the two built-in defaults.
