in-flight:


todo:

I'd like to enable delete protection on the access tokens table.


The frontend should respect some query string parameters
- auth_code, replaces the auth code currently stored in local storage
- join_room, automatically tries to join the room with that name on startup

Normal toaster errors can apply if, e.g. that auth_code is invalid (as checked by the /me endpoint) or if we can't successfully join that room

If we are the host of a room we should be able to generate a link that includes both concepts.

The auth_code selected for the link should be chosen such that:
- if we are using a non-admin code, use our code
- if we are using an admin code, find the most recent non-admin code using the admin code CRUD API and use that
- if no non-admin codes exist, error, do not create a URL with no auth code or with an admin auth code

The join_room should be chosen to be the room we're currently in.

When we're processing the auth_code, we should accept the new auth code if and only if we don't currently have a valid auth code ourselves. We should check our saved auth code for validity first, so that if our remembered code was revoked we replace it with the incoming code.


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
