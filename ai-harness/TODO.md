in flight:

I'd like to investigate styling options for the UI.

I'd like to know exactly what we have in place right now as far as theme / styling / css / etc.

I'd like to know what kind of options exist for adding styling to a project like this.

I'll want to eventually support light and dark modes, or even customizable themes.

We're just brainstorming here, don't write code yet. Feel free to do web searches. Feel free to launch subagents. If web searches fail fall back to curl or wget.


I want something like use_color_mode, but of our own invention, and it stores a whole theme settings document instead of just light vs dark.

That theme document will store all the unique colors we need under reasonable names. We should be able to extract some sensible defaults from our existing css code.

We should introduce a settings menu under a gear icon in the bottom left of the sidebar. This replaces the main content with a settings menu. Right now this settings menu is just a single dropdown with all the themes. Our theme list right now is just light and dark, extracted from our current css settings.

Using this dropdown updates a setting in our config for our preferred theme. We can default to the light theme if this setting is absent.

Config settings like this are key-value preferences stored in their own new table in our db. This will require a db migration to create the table.



todos:

more default themes
theme editor

options menu
one option to start, light/dark/system mode

needs spinner while waiting on response

ctrl+enter should submit

cost estimates? token estimates?

left sidebar is list of conversations
search box on top

main content is the conversation
markdown
images

tools, web search, bash?

auto compact

big document chunking and search, as a tool

MCP tools

we should be able to define tools that call out to other agents
e.g. anthropic model doing the main text of the conversation, but then it has a gemeni image tool
