in-flight:




todo:

json schemas for all API endpoint types and all websocket messages, then generate rust types from that


delete protection on access tokens table


I'd like to drop all the peer-to-peer stuff in the UI. Instead, we're going to implement a server-based system for running games.

The server is going to implement a combined REST and websocket API for managing clients.

We'll need two new dynamo tables:
- websocket-connections
   - primary key is a unique ID that identifies each active websocket connection
   - ttl that gets bumped as clients interact by sending websocket messages; 30 minutes
   - delete records immediately when a websocket connection dies
- rooms
   - primary key is a room name, chosen by the clients when they make rooms
   - stores game state
   - also stores an array of websocket connection IDs
   - also ttl, bump when anything happens on this room; 30 minutes

We need a REST API for listing rooms.

We need a websocket API that supports all the current peer-to-peer operations plus stuff like:
- create new room; errors if you try to create a room that already exists
- join room by name
- leave room by name
- various websocket messages that let the server respond to things, e.g. to update game state or to indicate room membership or to indicate an error
- all requests will include the access token

The UI should update the multiplayer menu in the following ways:
- if no access token is configured say so, have a button that hides multiplayer and presents the config menu where they can configure their access code
- otherwise, show the host room and join room buttons and the name text box
- we don't need the p2p copy token stuff any more, since all of that is replaced by the new APIs
- hosting or joining a room should start a new websocket connection if none exists, then try to send the appropriate messages

The server should keep track of game state in that table, and handle all incoming requests by:
- checking whether this access token is allowed to make changes (i.e. it's readwrite or readonly)
- if this is a legal move to make to update the game state
- actually update the game state on a successful message, and then emit state update messages to all connected clients


frontend should respect some query string parameters
- auth_code, replaces the auth code currently stored in local storage
- room, automatically tries to join the room with that name on startup

Normal toaster errors can apply if, e.g. that auth_code is invalid (as checked by the /me endpoint) or if we can't successfully join that room



I'd like to review all the teaching tooltips for accuracy. Look them up in the book by the reference pages, check for accuracy, and verify that they make sense for that wigdet. Split off as many subagents as necessary.


A previous bit of investigation revealed some possible improvements we can make later. I want to review how much these still matter, or whether they make sense at all
  2. + Defense tracking
     Adds Dodge DV / Parry DV as entered numbers, live DV penalty from the current action, DV refresh timing, and per-attacker onslaught counters. Still no attack or damage resolution.
  3. + Attacks & health
     Full Chapter Four loop: weapons, accuracy, soak, damage, health track and wound penalties. Much larger; the weapon tables in RULES.md are OCR-damaged and would need verification
     against the books first.


dice roller
