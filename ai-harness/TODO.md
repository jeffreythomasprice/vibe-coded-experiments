in flight:

I want to introduce embeddings models alongside the existing normal llm models for each provider. There should be a new interface for embeddings provider, similar to the existing ChatProvider and ImageProvider. Any provider that hosts any embeddings models should implement this interface. 

It should have a similar function to the other providers for listing all possible model ids. It should be convenient for the user to determine how big a string can be passed to each model and how big the resulting vector will be. If it's hard to determine that ahead of time, we should at least produce a sensible error type when the input string is too big. This kind of information can be in the embeddings version of ModelDetails, or as an extension to the existing ModelDetails, depending on how we implement this feature.

We should write unit tests for any new functionality that makes sense to unit test, and integration tests for actually invoking the real providers.

We should expand the demo page to also include embeddings models for each provider. There should be something on each row to indicate whether it's a chat model or an embeddings model. At the top of each section should be a dropdown for: Show All, Show Chat Only, Show Embeddings Only. For Ollama only there should be a checkbox for Show Local Only that omits models not already pulled locally.



todos:

config.toml shouldn't bake default model ids in. When we need specific models for integration tests we can bake them into constants in those files. For all other purposes we should require that the caller provide that as part of the request.



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
