game/engine needs a database. We should use turso, and we should have some kind of database migration system. For now we don't need actually store anything in the database, a placeholder initial migration is fine.

The database location should in the config file, and should default to ~/.config/munchkin/engine.db


game/engine and game/tui need to be able to communicate. We should implement a request-response system via unix sockets. The payloads can be json, and should be in game/shared.

If the client can't connect to the engine on startup it can log about it and exit non-0.

The engine can't start listening on the unix socket on startup it can log about it and exit non-0.

The exact protocol of the messages will get more involved as we implement real functionality, so it can be fairly stub-like for now.
- client hello -> server response to hello
- client ping -> server response to ping
- client request server stats -> server response, includes client list with their last ping times

Server can kick clicks that haven't pinged in a while (2 minutes).


tui needs to be a real tui


markdown in various text fields? at least bold and italics


make a card renderer
assets/raw/font/Windlass.ttf