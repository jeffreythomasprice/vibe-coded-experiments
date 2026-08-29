in flight:



todos:

figure out a proper way for it to test the real ui
it keeps trying to do weird run it and screenshot strategies that don't work

proper integration tests?

should be able to edit agents

should be able to delete conversations
both in the all conversations screen
and a right click menu with a delete option in the sidebar

why are vfs methods sync? isn't there async file io we should be doing?

collapse all the db migrations down to one

automatic tool approve rules
e.g. regex on bash commands

add files to a conversation
add files to a project?
just adding to an existing conversation adds to the effective project an ad-hoc "files" directory where it copies the files?

scratch dir, plus system prompt for writing little scripts there?

standardize project and agent behavior re. copying into converstaions vs foreign key
currently agent is copied into conversation, but project is foreign key linkage

cost estimates? token estimates?

search box for conversations

support images in output

tools, web search, bash?

auto compact

big document chunking and search, as a tool

MCP tools

we should be able to define tools that call out to other agents
e.g. anthropic model doing the main text of the conversation, but then it has a gemeni image tool
