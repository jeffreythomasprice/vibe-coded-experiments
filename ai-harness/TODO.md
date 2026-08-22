in flight:

I want to support .env files. There should be an example file committed to source control, and we should .gitignore the actual .env file.

The sample should show have a sample for both the anthropic and openai keys.

The README.md should have some instructions for setting this up somewhere near the top of the file.




todos:

left sidebar is list of conversations
search box on top

main content is the conversation
markdown
images

modules:
- AI router stuff
	- for each provider, list possible models
	- for ollama list both locally pulled models, and the full catalog of possible models
- tools, web search, bash?
- agents = model + system prompt + list of tools
- auto compact
- embeddings models, big document chunking and search, as a tool
- MCP tools

we should be able to define tools that call out to other agents
e.g. anthropic model doing the main text of the conversation, but then it has a gemeni image tool
