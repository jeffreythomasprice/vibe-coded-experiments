import pdf from "pdf-parse";
import { MDocument } from "@mastra/rag";
import logger from "./logger.js";
import { CHUNK_SIZE, CHUNK_OVERLAP } from "./config.js";

export async function extractText(filepath: string): Promise<string> {
  logger.debug({ filepath }, "extracting text");
  const ext = filepath.split(".").pop()?.toLowerCase();
  if (ext === "pdf") {
    return extractPdf(filepath);
  }
  return Bun.file(filepath).text();
}

async function extractPdf(filepath: string): Promise<string> {
  const buffer = Buffer.from(await Bun.file(filepath).arrayBuffer());
  const data = await pdf(buffer);
  return data.text;
}

export async function chunkText(
  text: string,
  chunkSize: number = CHUNK_SIZE,
  overlap: number = CHUNK_OVERLAP,
): Promise<string[]> {
  const doc = MDocument.fromText(text);
  const chunks = await doc.chunk({
    strategy: "recursive",
    maxSize: chunkSize,
    overlap,
  });
  return chunks.map((c) => (typeof c === "string" ? c : c.text));
}
