big features:
- multiple people in the same environment
- chat
- map
- minis on map
- effects, e.g. explosions
- map has graphics, simple colored shapes but also textures

stub our a multi-project rust workspace:
- server
	- http endpoints
	- websockets for real time updates to clients
	- env files that determine what ip to bind to and port, defaults to localhost:8001
	- logging via tracing, our stuff on trace, other packages on warn
- client
	- this is a leptos web ui
	- connects to server
	- env files that determine where server lives, default committed to source control will be the localhost:8001
	- leptos dev server runs on localhost:8000
	- logging, our stuff on trace, other packages on warn
- shared
	- data types for http and websocket requests



The server needs the following:
- a config file; search for this in these locations, in this order, first wins:
	- path provided by cli arg, which is optional
	- $CWD/config.toml
	- ~/.config/roll20-clone/config.toml
- an example config file should be committed to source control that has sensible defaults
- config file contains the following:
	- log file location, example config file has /tmp/roll20-clone/log
- server logs both to stderr and that file
- logs to file should be rotated at max size or time (e.g. 10 mb, or daily, whichever comes first, keep 30 logs)
- only one copy of the server can run at once, use a lock file in /tmp/roll20-clone
- if we exit because another copy of server is running we should abort with non-0 exit code
- turso db
- location of db is in config file, example config file has ~/.config/roll20-clone/db
