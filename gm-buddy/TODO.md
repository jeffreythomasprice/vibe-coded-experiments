merge llm-rag into this
maybe also gcs-ai-skill?

mcp and new skills: document storage
reuse parts of llm-rag
text and pdf
chunking
vector store
mcp for searching chunks and for getting arbitrary sections by byte index or page numbers (pdf only)
skill for answering questions based on document store
skill for summarizing document store and producing new documents/memories?

mcp and new skills: info node graph
arbitrary node and edge types
each node and edge can have arbitrary text associated with it
each node has a type string
mcp would be ways to create locations, or things by type that have a relationship (e.g. get all items in this location)
skill that gives examples of a use case like:
- nodes could be locations, items, people, events, clues
- edges could be location-location with a description of relative direction or how to get there, location-people "contains", location-event "happens in"



phase 1: tool that serves as both it's cli and server, like how llm-rag behaves now
- no tui, only cli
- stores documents, with arbitrary list of tags
- documents are arbitrary text, optionally with page number ranges (pdf only)
- you can get an arbitrary range of text (either byte byte range or page range), and if it was pdf the result will include page numbers
- list documents, optionally filtering by name or tags
- documents are chunked and vector searchable
- if you update a document it automatically redoes the chunking and vectorizing
- cli commands default to waiting for response, but it works with a queue internally and you can say don't wait
- other cli commands for looking at the queue

phase 2: mcp and skills for document store
- ingest new documents
- find documents
- use document store as a memory system, use a fixed tag for memory
- skill hooks for stuff like: remember this, forget this, find answer to question in documents

phase 3: mind map
- new kind of data that can be stored alongside documents: nodes and edges
- nodes and edges both can have tags
- edges link two nodes
- both can have names and arbitrary descriptive text (string? array of string?)
- cli commands for crud, including stuff like "find all nodes linked to this node"
- mcp and skills for those
