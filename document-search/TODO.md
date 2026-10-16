in flight:

document-search ingest --detach --force-ocr --ocr-language jpn_vert+jpn --summary-lang en ~/scratch/books/Final\ Fantasy\ IV\ \(Volume\ 2\).pdf
document-search ingest --detach --force-ocr --ocr-language jpn_vert+jpn --summary-lang en ~/scratch/books/Final\ Fantasy\ IV\ \(Volume\ 1\).pdf

document-search summarize --rebuild --detach "/home/jeff/scratch/books/Final Fantasy IV (Volume 1).pdf" --summary-lang en
document-search summarize --rebuild --detach "/home/jeff/scratch/books/Final Fantasy IV (Volume 2).pdf" --summary-lang en

tail -f /tmp/document-search.log

document-search delete '/home/jeff/scratch/books/Final Fantasy IV (Volume 1).pdf'
document-search delete '/home/jeff/scratch/books/Final Fantasy IV (Volume 2).pdf'




todo:

translation, not just english summaries
