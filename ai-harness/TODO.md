in flight:

I want to build a simple UI that touches the agent and conversation system.

The main interface should be a sidebar on the left and a main content area on the right.

When the program starts the main content area is blank.

The sidebar shows the following items, in this order:
- New
- Agents
- The most recent 20 conversations, organized by most recently updated to least
- View all conversations

Clicking a conversation puts the conversation content in the main area. The formatting of each kind of message varies:
- human messages are in a "bubble" and moved to the right
- AI messages are full width, and should be run through a markdown-to-html formatter
- tool messages should have a summary of what tool was used but otherwise hide the full details in a collapsable section; these should also be "bubbles" but left justified
- thinking messages can be collapsable bubbles like tool use

At the top of each kind of message, regardless of formatting, should be a timestamp.

Clicking on the "View all conversations" item in the sidebar should replace the main content with a full list of all conversations, sorted by most recently updated to least.

Clicking on the "New" item in ths sidebar should replace the conversation with a new conversation. This should present the user with a dropdown to select which agent. There should be a button to create a new agent.

Clicking to create a new agent presents the user with an agent form where they are required to input:
- a name for the agent
- a dropdown for provider; this should be a dropdown, but also where you can type and it tries to autocomplete
- once they select a provider, a dropdown for model; also a dropdown / autocomplete text field
- a system prompt, big text area
- Save / Cancel

Once they've selected an agent, entered some message in the text area, and submitted it, that agent and conversation should now be locked in. The conversation is in the db, and has that agent associated with it.

If they select "Agents" from the sidebar the main content is replaced by a list of all agents. Next to each item is a delete button. There is a new button, that presents the same new agent form as when they do this inline with a conversation.

Agent settings should get copied to conversations in the db when they are used. Agents are therefore templates for new conversations more than they are foreign key relationships. This may require a db change. We can edit the existing migrations, no existing db exists to update.


todos:


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
