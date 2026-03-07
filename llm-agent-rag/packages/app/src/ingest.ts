import chalk from "chalk";
import path from "path";
import type { IngestResult } from "@rag/shared";
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

  console.log(chalk.bold("Ingesting:"), filepath);

  // 1. Extract text
  console.log("  Extracting text...");
  const text = await extractText(filepath);
  if (!text.trim()) {
    throw new Error(`No text extracted from ${filepath}`);
  }
  console.log(`  Extracted ${text.length.toLocaleString()} characters`);

  // 2. Chunk
  const chunks = await chunkText(text);
  console.log(`  Split into ${chunks.length} chunks`);

  // 3. Embed (in batches)
  console.log("  Generating embeddings...");
  const allEmbeddings: number[][] = [];
  for (let i = 0; i < chunks.length; i += EMBED_BATCH_SIZE) {
    const batch = chunks.slice(i, i + EMBED_BATCH_SIZE);
    const batchEmbeddings = await embedTexts(batch);
    allEmbeddings.push(...batchEmbeddings);
  }

  // 4. Store
  console.log("  Storing in database...");
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
  console.log(
    chalk.green("  Done!"),
    `document_id=${docId}, ${chunks.length} chunks stored`,
  );
  return summary;
}
