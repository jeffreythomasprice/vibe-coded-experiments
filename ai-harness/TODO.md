in flight:

I want to introduce a logging and config system.

All modules should be able to log, and we should be able to see in the log line which module it came from, and which file ideally. Timestamps should all be in ISO8601 format, with at least millisecond precision. The default log level should be trace level for all of our code, but default all other packages to warn.

We should use a log rotation system. We'll be logging to a directory. Each log file will have some common naming convention, which will include a timestamp of the rotation time. We log at some fixed time interval or a max file size, and keep a max number of such files.

Our config system should be a toml configuration file that is read from the filesystem. We read from these locations, in priority order, first wins:
- if provided, optiona cli arg to define config file location
- ~/.config/ai-harness/config.toml

The config file can start with just some properties about the logger:
- log dir location, which should default to /tmp/ai-harness/logs
- log rotation parameters, defaults to daily or 100mb, whichever comes first, and then keep last 10

We should log the contents of the config file and where we found it on startup.



todos:

/plugin install frontend-design@claude-code-plugins

left sidebar is list of conversations
search box on top

main content is the conversation
markdown
images

modules:
- frontend
	- leptos, tauri
- backend
- lib
	- AI router stuff
		- ollama
		- anthropic
		- somebody with good image gen, gemeni?
	- tools, web search, bash?
	- agents = model + system prompt + list of tools
	- auto compact
	- embeddings models, big document chunking and search, as a tool
	- MCP tools
- shared
	- conversation and message types

we should be able to define tools that call out to other agents
e.g. anthropic model doing the main text of the conversation, but then it has a gemeni image tool
