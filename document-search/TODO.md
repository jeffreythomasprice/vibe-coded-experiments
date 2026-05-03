test:
- ingest a file (txt and pdf)
- get ranges from a file (bytes, chars, pages)
- do vector search on a file

dynamic chunking instead of fixed regions?

skill






I want to be able to do vector search using a search term and find matching chunks.

The default number of matching chunks per document is 5, but it's configurable.

I want to be able to specify a single document by exact path, or a set of documents by providing a list of tags. If providing tags the default behavior is that matching documents must have all tags, but there should be a cli flag to switch that to an "any of" behavior.

When multiple documents are involved in the search we should find the desired number of matching chunks in each document, then try to combine results.

Whether there is a single document or multiple, we should have a relevancy cutoff, below which we ignore those results. The relevancy cutoff is also configurable.