I want to introduce proper gameplay to game/engine.

We should track game state as a thing in the database. There can be multiple games ongoing at once, and games can be in different states, i.e. the different phases in the turn order and whose turn it is.

Players can be either AI or human.

AI players will be LLMs. We'll just use ollama for now. The exact ollama connection host and port, and the model id, can all be in the config. Default to localhost at whatever the default ollama port is. Default to qwen3.5 for model id.

When it's an AI player's turn we'll provide a summary of the current situation to the LLM and solicit the action. e.g. if they drew a non-monster when kicking down the door, do they intend to player a card from their hand, etc. For human players we will pause indefinitely and wait until they submit a message for their action.

There will need to be interjection points at appropriate places. e.g. if a player is facing a monster, that player and the other players have the opportunity to play cards. For AI players we'll provide a summary of what is going on as the prompt and solicit whether they intent to play a card, or offer to help fight the monster, etc. For human players we'll pause for a short delay (5 seconds, configurable in config.toml) and wait for that human player to send a message to the engine about their action.

We'll need appropriate message types defined in game/shared for all possible kinds of actions, and for providing information about current game state.

Connected clients will need to provide which player they represent, if any. A connected client might just be observing an ongoing game.

The TUI will need updating to provide a game menu. We should be able to see all the ongoing games. Selecting one should let us join that game as an observer, or as a player. The engine should guarantee that only one client can be connected as a particular player at a time. If there are multiple human players, multiple instances of the tui client should be connected as different players. If a tui client is already connected as a particular player, a second shoudl be rejected. The TUI should be able to show error messages from the server for this kind of situation.


go back through the rules and make sure we cover every possible card in at least basic details
AI quorum voting on rules layering?


markdown in various text fields? at least bold and italics


make a card renderer
assets/raw/font/Windlass.ttf