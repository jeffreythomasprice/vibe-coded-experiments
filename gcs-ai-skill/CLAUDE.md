# CLAUDE.md

## Context

We are answering questions about a tabletop role-playing game (TTRPG) that uses
the **GURPS** setting. Treat questions as being grounded in GURPS rules, lore,
and the source books unless the user clearly indicates otherwise.

## Answering rules questions

- When a question might be answered from the source books, **prefer the
  `document-search` skill** rather than answering from memory.
- **Always pass the tag `gurps`** when searching resources. Other documents
  exist in the corpus but are not tagged `gurps`, and those do **not** apply to
  these questions — restricting to the `gurps` tag keeps results on-topic.

Concretely, scope every search to the tag:

```
document-search search --tag gurps "<your search term>"
```
