import chalk from "chalk";
import path from "path";
import type { IngestResult } from "@rag/shared";
import logger from "./logger.js";
import { print } from "./output.js";
import { insertDocument, insertTags, insertChunks, closeSql } from "./db.js";
import { extractText, chunkText } from "./text.js";
import { embedTexts } from "./embeddings.js";

const EMBED_BATCH_SIZE = 32;

export async function ingestFile(
  filepath: string,
  tags: Record<string, string> = {},
  overrideName?: string,
): Promise<IngestResult> {
  filepath = path.resolve(filepath);
  const filename = overrideName ?? path.basename(filepath);

  tags = { filename, ...tags };

  logger.info({ filepath, tags }, "ingesting file");
  print(chalk.bold("Ingesting:"), filepath);

  // 1. Extract text
  print("  Extracting text...");
  const text = await extractText(filepath);
  if (!text.trim()) {
    throw new Error(`No text extracted from ${filepath}`);
  }
  logger.info({ filepath, characters: text.length }, "text extracted");
  print(`  Extracted ${text.length.toLocaleString()} characters`);

  // 2. Chunk
  const chunks = await chunkText(text);
  logger.info({ filepath, chunks: chunks.length }, "text chunked");
  print(`  Split into ${chunks.length} chunks`);

  // 3. Embed (in batches)
  logger.info({ filepath, chunks: chunks.length, batchSize: EMBED_BATCH_SIZE }, "generating embeddings");
  print("  Generating embeddings...");
  const allEmbeddings: number[][] = [];
  for (let i = 0; i < chunks.length; i += EMBED_BATCH_SIZE) {
    const batch = chunks.slice(i, i + EMBED_BATCH_SIZE);
    const batchEmbeddings = await embedTexts(batch);
    allEmbeddings.push(...batchEmbeddings);
  }

  // 4. Store
  logger.info({ filepath, filename }, "storing in database");
  print("  Storing in database...");
  const docId = await insertDocument(filename);
  await insertTags(docId, tags);
  await insertChunks(
    docId,
    chunks.map((content, i) => ({
      chunkIndex: i,
      content,
      embedding: allEmbeddings[i],
    })),
  );

  const summary = {
    document_id: docId,
    filename,
    characters: text.length,
    chunks: chunks.length,
    tags,
  };
  logger.info({ docId, filename, chunks: chunks.length }, "ingest complete");
  print(
    chalk.green("  Done!"),
    `document_id=${docId}, ${chunks.length} chunks stored`,
  );
  return summary;
}
