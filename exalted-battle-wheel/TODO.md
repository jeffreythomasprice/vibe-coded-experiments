in-flight:


todo:

server is a docker container
kubernetes hosting, see ../kubernetes-host/example for how to deploy to my host
rust app
dynamodb storage
docker-compose with local dynamo for testing
one table + api for users, both login and user crud
one table + api for rooms, rooms can have people in them, scanning for rooms to join, starting a room
websocket api for receiving room updates, sending join and leave commands, etc.
update UI to include login page, user crud for admins
server is doing the room update validation, sending invalid updates rejects
drop the entire p2p system since we now use our own host for communication


I'd like to review all the teaching tooltips for accuracy. Look them up in the book by the reference pages, check for accuracy, and verify that they make sense for that wigdet. Split off as many subagents as necessary.


A previous bit of investigation revealed some possible improvements we can make later. I want to review how much these still matter, or whether they make sense at all
  2. + Defense tracking
     Adds Dodge DV / Parry DV as entered numbers, live DV penalty from the current action, DV refresh timing, and per-attacker onslaught counters. Still no attack or damage resolution.
  3. + Attacks & health
     Full Chapter Four loop: weapons, accuracy, soak, damage, health track and wound penalties. Much larger; the weapon tables in RULES.md are OCR-damaged and would need verification
     against the books first.


dice roller
