I'd like to implement a multi-project rust workspace here, with the first projects in it being a Munchkin game engine. We should have two projects:
- a game engine, holding most code
- a library that just holds DTOs, messages, etc. intended for transferring state to client apps we may make in the future

We don't need a client yet, we'll do that later. All functionality should be covered by unit tests. We don't need any explicit wire transfer yet (e.g. no HTTP server), just a set of useful public functions for interacting with the game state.

We should implement the game as described by assets/processed/rules.md

I'd like to support both AI and human players, in any combination (e.g. all AI or all human or a mix are all allowed).

A game should have the concept of whose turn it is, and where we are in that turn. If we're waiting on human input we might just pause there until the player's input is provided by some public function. If we need AI input we can invoke the AI code directly and get the response.

AI's should be implemented by an LLM. We'll just support ollama for now.

All config should be in a config.toml that should be located by checking, in order:
- the location specified by an optional cli arg --config
- the current working directory, config.toml
- ~/.config/munchkin/config.toml

We should log where we got our config from, and it's contents. We should use tracing for logging.


game engine needs an HTTP server
HTTP API + websockets


game engine needs to be able to represent multiple ongoing games at once in a db
db might be in-memory for a self-contained app, or might be a postgres db or something


markdown in various text fields? at least bold and italics


make a card renderer
assets/raw/font/Windlass.ttf