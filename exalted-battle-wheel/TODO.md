in-flight:


todo:

THE LAMBDA WEBSOCKETS PLAN:
everybody connects to websocket
host sends websocket message saying they want to start a room
others send websocket message saying they want to join a room
store active websocket connections and what rooms they're in in dynamo
when somebody takes an action that's a websocket message, lambda validates that it's valid, computes new state, sends update to every connected client
dynamo ttls + client timeouts when no action is taken in a room for a while

THE LAMBDA STUN WEBRTC PLAN:
lambda + dynamo
host polls a lambda to see if anybody wants to join
joiners poll a lambda until they get connected
lambda responds with stun info, the host and client respond with their most up to date stun results, repeat until they get connected

THE KUBERNETES PLAN:
cheapest possible ec2 instance running a mini kubernetes (k3s? k0s? minikube?)
runs a server as a pod, singleton, as much as possible in-memory
clients connect to server via websockets, rooms are in memory
user management probably in dynamo for auth
server handles all room state checking, sends updates via websockets to all people in that room




I'd like to review all the teaching tooltips for accuracy. Look them up in the book by the reference pages, check for accuracy, and verify that they make sense for that wigdet. Split off as many subagents as necessary.


A previous bit of investigation revealed some possible improvements we can make later. I want to review how much these still matter, or whether they make sense at all
  2. + Defense tracking
     Adds Dodge DV / Parry DV as entered numbers, live DV penalty from the current action, DV refresh timing, and per-attacker onslaught counters. Still no attack or damage resolution.
  3. + Attacks & health
     Full Chapter Four loop: weapons, accuracy, soak, damage, health track and wound penalties. Much larger; the weapon tables in RULES.md are OCR-damaged and would need verification
     against the books first.


dice roller
