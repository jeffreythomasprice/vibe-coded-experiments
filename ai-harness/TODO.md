in flight:

I want to make sure that our llm provider and our agent system all use sensible error result types. We should have error enums that cover common cases like "embeddings string input is too big" and "you need to pay more to the provider"


todos:

I want to set up a turso database. The file location should be in config.toml and default to some filename in /tmp/ai-harness

We should introduce sqlx and some migration system. The migration system needs to support both pure sql and rust functions.

The initial state of the db should be:
- agent configs
- conversations, including all messages, tool use, which agent, etc.


cost estimates? token estimates?

left sidebar is list of conversations
search box on top

main content is the conversation
markdown
images

modules:
- tools, web search, bash?
- agents = model + system prompt + list of tools
- auto compact
- embeddings models, big document chunking and search, as a tool
- MCP tools

we should be able to define tools that call out to other agents
e.g. anthropic model doing the main text of the conversation, but then it has a gemeni image tool
