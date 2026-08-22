in flight:


todos:

I want to introduce new abstractions on top of the llm providers in lib. Presumably all this code will also go in either lib, or shared as needed to allow the frontend to eventually interact with this system.

I want an LLM provider router. There should be some generic way of representing an arbitrary provider.

I want an "agent" system, where an agent is defined as the following:
- a particular model provider with some model selected
- a system prompt, expressed as one or more strings
- a list of tools

We should have a generic tool concept that can apply to any model provider. It should be easy to provide new tools via either rust funcs or by implementing some interface.

It should be convient to create a new agent from those pieces, e.g. via convenience functions for making quick tools, builder patterns, or similar.

We should write unit tests that cover all the new behavior. As normal for unit tests we don't need to use real network or real providers here. Existing integration tests already cover the actual real providers functionality.




sensible error types for things like "embeddings string input is too big" and "you need to pay more to the provider"

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
