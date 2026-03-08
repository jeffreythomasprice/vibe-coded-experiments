import chalk from "chalk";
import path from "path";
import type { IngestResult } from "@rag/shared";
import logger from "./logger.js";
import { print } from "./output.js";
import { insertDocument, insertTags, insertChunks, closeSql } from "./db.js";
import { extractText, chunkText, mapChunksToPages } from "./text.js";
import { embedTexts } from "./embeddings.js";

const EMBED_BATCH_SIZE = 32;

export async function ingestFile(
  filepath: string,
  tags: string[] = [],
  overrideName?: string,
): Promise<IngestResult> {
  filepath = path.resolve(filepath);
  const filename = overrideName ?? path.basename(filepath);

  tags = [`filename=${filename}`, ...tags];

  logger.info({ filepath, tags }, "ingesting file");
  print(chalk.bold("Ingesting:"), filepath);

  // 1. Extract text
  print("  Extracting text...");
  const extracted = await extractText(filepath);
  const text = extracted.text;
  if (!text.trim()) {
    throw new Error(`No text extracted from ${filepath}`);
  }
  logger.info({ filepath, characters: text.length }, "text extracted");
  print(`  Extracted ${text.length.toLocaleString()} characters`);

  // 2. Chunk
  const chunksWithOffsets = chunkText(text);
  const chunkTexts = chunksWithOffsets.map(c => c.text);
  logger.info({ filepath, chunks: chunksWithOffsets.length }, "text chunked");
  print(`  Split into ${chunksWithOffsets.length} chunks`);

  // 3. Embed (in batches)
  logger.info({ filepath, chunks: chunksWithOffsets.length, batchSize: EMBED_BATCH_SIZE }, "generating embeddings");
  print("  Generating embeddings...");
  const allEmbeddings: number[][] = [];
  for (let i = 0; i < chunkTexts.length; i += EMBED_BATCH_SIZE) {
    const batch = chunkTexts.slice(i, i + EMBED_BATCH_SIZE);
    const batchEmbeddings = await embedTexts(batch);
    allEmbeddings.push(...batchEmbeddings);
  }

  // 4. Store
  logger.info({ filepath, filename }, "storing in database");
  print("  Storing in database...");
  const docId = await insertDocument(filename);
  await insertTags(docId, tags);
  // Map chunks to page numbers if page boundaries are available
  const pageMap = extracted.pageBoundaries
    ? mapChunksToPages(chunksWithOffsets, extracted.pageBoundaries)
    : null;

  await insertChunks(
    docId,
    chunkTexts.map((content, i) => ({
      chunkIndex: i,
      content,
      embedding: allEmbeddings[i],
      pageStart: pageMap ? pageMap[i].pageStart : null,
      pageEnd: pageMap ? pageMap[i].pageEnd : null,
    })),
  );

  const summary = {
    document_id: docId,
    filename,
    characters: text.length,
    chunks: chunkTexts.length,
    tags,
  };
  logger.info({ docId, filename, chunks: chunkTexts.length }, "ingest complete");
  print(
    chalk.green("  Done!"),
    `document_id=${docId}, ${chunkTexts.length} chunks stored`,
  );
  return summary;
}
