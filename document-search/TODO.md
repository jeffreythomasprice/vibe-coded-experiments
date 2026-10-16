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



I want to export these to markdown and pdf files to a new tmp directory in this directory:
'/home/jeff/scratch/books/Final Fantasy IV (Volume 1).pdf'
'/home/jeff/scratch/books/Final Fantasy IV (Volume 2).pdf'

We should translate to english.




todo:

editing pass that does things like check whether quotes start on new lines, try to fix grammar with commas and quotes etc.