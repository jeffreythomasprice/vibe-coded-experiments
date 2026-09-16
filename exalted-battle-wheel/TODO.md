in-flight:

I want to expand the server with a dynamo db table that stores access codes.

I want a local dynamo that I can launch via a docker compose file. Instructions for doing this should go in the local testing section of the readme.

I want terraform to deploy the dynamo table for real as part of deployment.

The contents of the table should be:
- accessKey, string, primary key
- isAdmin, boolean
- createdAt, an ISO8601 timestamp for when it was created

I want an API in the server that can check whether a particular access code is valid. This should return the metadata about the access token (isAdmin, createdAt)

I want additional APIs that do CRUD on access codes. These APIs are available only for admin access codes, and should 403 otherwise.

We should prepare a sensible service layer for interacting with this, e.g. helpers for returning the same kind of error early if the incoming access code is missing, or if it requires admin and we don't have an admin access code.

We expect access codes to be Bearer tokens in the Authorization header. We should use axum middleware as appropriate.

I want some examples in the README.md for how to set up the initial admin access code in either the real dynamo db or in the local one, and then some curl for interacting with the real API to do CRUD on access tokens.

My goal at the end of this is a server I can test locally or deploy, prove authentication and authorization work against this access token API.


todo:

for local testing, there should be a script for launching everything, shutting everything down when ctrl+c
also should make dynamo tables
sync dynamo table with terraform scripts even for local to avoid duplicating table definition?


json schemas for all API endpoint types and all websocket messages, then generate rust types from that


delete protection on access tokens table


The UI should have a config gear icon in the top right. This presents a modal dialog that shows either:
- a prompt to configure the current access token
- metadata about the current access token, and a button to clear it

If the current access token is an admin, this should also present a table view of all access tokens with their metadata, sorted by create date. An X on each row should let you delete it. A checkbox on each row should let us toggle whether this token is an admin token or not. A create form at the bottom should let you type any string and submit a new access token. Deletes should use a confirmation modal.

All API actions should present a common spinner and disable the rest of the forms while that action is taking place.

My goal at the end of this is a UI that can fully exercise the server's access token API (checking status of a token, plus full CRUD on tokens).


drop all of the p2p stuff, keep only messages that make sense to repurpose as websocket messages to server
replace with a dynamo table for rooms
REST API for CRUD on rooms
websocket API for joining/leaving rooms
everybody in a room gets all updates when something happens in a room
server keeps track of room state and updates when update messages come in
everybody in a room can be readonly or readwrite, readwrite can flip status for everybody but themselves or the host
the host is always readwrite
new rooms can be "everybody is readwrite by default" or "only host is readonly by default"
keep track of individual websocket connections by some ID, which goes in it's own dynamo table, with a ttl
contents of the websocket connections table are which rooms they are in
rooms table keeps track of the actual game state, and also has a list of all websocket IDs that are in that room


I'd like to review all the teaching tooltips for accuracy. Look them up in the book by the reference pages, check for accuracy, and verify that they make sense for that wigdet. Split off as many subagents as necessary.


A previous bit of investigation revealed some possible improvements we can make later. I want to review how much these still matter, or whether they make sense at all
  2. + Defense tracking
     Adds Dodge DV / Parry DV as entered numbers, live DV penalty from the current action, DV refresh timing, and per-attacker onslaught counters. Still no attack or damage resolution.
  3. + Attacks & health
     Full Chapter Four loop: weapons, accuracy, soak, damage, health track and wound penalties. Much larger; the weapon tables in RULES.md are OCR-damaged and would need verification
     against the books first.


dice roller
