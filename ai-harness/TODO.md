in flight:

I want to make sure that our llm provider and our agent system all use sensible error result types. We should have error enums that cover common cases like "embeddings string input is too big" and "you need to pay more to the provider"


todos:

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
