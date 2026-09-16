in-flight:



todo:

is there a bug where the room host disconnects?
can we fix reconnects? tokens issued for an active webbrowser session, in session storage?


json schemas for all API endpoint types and all websocket messages, then generate rust types from that


delete protection on access tokens table


frontend should respect some query string parameters
- auth_code, replaces the auth code currently stored in local storage
- room, automatically tries to join the room with that name on startup

Normal toaster errors can apply if, e.g. that auth_code is invalid (as checked by the /me endpoint) or if we can't successfully join that room


frontend should have a UI that exposes all rooms
admins can close rooms early
join room by searching and clicking


I'd like to review all the teaching tooltips for accuracy. Look them up in the book by the reference pages, check for accuracy, and verify that they make sense for that wigdet. Split off as many subagents as necessary.


A previous bit of investigation revealed some possible improvements we can make later. I want to review how much these still matter, or whether they make sense at all
  2. + Defense tracking
     Adds Dodge DV / Parry DV as entered numbers, live DV penalty from the current action, DV refresh timing, and per-attacker onslaught counters. Still no attack or damage resolution.
  3. + Attacks & health
     Full Chapter Four loop: weapons, accuracy, soak, damage, health track and wound penalties. Much larger; the weapon tables in RULES.md are OCR-damaged and would need verification
     against the books first.


dice roller
