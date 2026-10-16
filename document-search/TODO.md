in flight:

document-search ingest --detach --force-ocr --ocr-language jpn_vert+jpn --summary-lang en ~/scratch/books/Final\ Fantasy\ IV\ \(Volume\ 1\).pdf
document-search ingest --detach --force-ocr --ocr-language jpn_vert+jpn --summary-lang en ~/scratch/books/Final\ Fantasy\ IV\ \(Volume\ 2\).pdf

document-search summarize --rebuild --detach "/home/jeff/scratch/books/Final Fantasy IV (Volume 1).pdf" --summary-lang en
document-search summarize --rebuild --detach "/home/jeff/scratch/books/Final Fantasy IV (Volume 2).pdf" --summary-lang en

tail -f /tmp/document-search.log

document-search info '/home/jeff/scratch/books/Final Fantasy IV (Volume 1).pdf'
document-search info '/home/jeff/scratch/books/Final Fantasy IV (Volume 2).pdf'

document-search delete '/home/jeff/scratch/books/Final Fantasy IV (Volume 1).pdf'
document-search delete '/home/jeff/scratch/books/Final Fantasy IV (Volume 2).pdf'




todo:

Summarize these documents:
'/home/jeff/scratch/books/Final Fantasy IV (Volume 1).pdf'
'/home/jeff/scratch/books/Final Fantasy IV (Volume 2).pdf'


I'd like to be able to output a complete text document of an ingested document.

We don't need to try to preserve any formatting from the original pdf, e.g. if there are graphs or tables or something. Those kinds of artifacts can become broken text fragments in the resulting output, just as they would be in the OCR data.

We do need to optionally try to convert the langauge. If we're doing this the input command should require a target langauge. We will then run the document through an llm with a system prompt that says something like "<document>...</document> Convert this document to {language}". Since doing the whole document in one go is probably not going to work, we should do a small number of chunks at a time, with adjacent segments overlapping by at least one chunk. The number of chunks at a time and the amount of overlap should be configurable with some sensible default in our config. A second llm pass can be used on adjacent segments to try to de-duplicate or smooth the resulting text.

If it's possible to assume any formatting from the input, we should provide a markdown format. e.g. if there are section headers or similar we could use markdown header tags. We should also support pdf output, which is just the markdown rendered to pdf.

We should make sure to update REAMDE.md with usage example and CLAUDE.md with more details.

So the full feature set is something like:
- txt, md, or pdf output
- either language as-written, or optionally translate in chunks by llm