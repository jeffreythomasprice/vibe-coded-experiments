import { createOllama } from "ollama-ai-provider-v2";
import { generateText } from "ai";
import type { ChunkResult, AskResponse } from "@rag/shared";
import logger from "./logger.js";
import { CHAT_MODEL, CONTEXT_WINDOW, OLLAMA_BASE_URL } from "./config.js";
import { fetchContextChunks, searchChunks } from "./db.js";
import { embedSingle } from "./embeddings.js";

function expandResultsWithContext(
  results: ChunkResult[],
  window: number,
): Promise<ChunkResult[]> {
  if (window <= 0) return Promise.resolve(results);

  // Group results by document_id
  const byDoc = new Map<number, ChunkResult[]>();
  for (const r of results) {
    if (!byDoc.has(r.document_id)) byDoc.set(r.document_id, []);
    byDoc.get(r.document_id)!.push(r);
  }

  // For each document, compute merged index ranges
  const docExpanded = new Map<number, Map<string, string>>();

  const tasks: Promise<void>[] = [];

  for (const [docId, docResults] of byDoc) {
    // Build ranges
    const ranges: [number, number][] = docResults.map((r) => [
      Math.max(0, r.chunk_index - window),
      r.chunk_index + window,
    ]);
    ranges.sort((a, b) => a[0] - b[0]);

    // Merge overlapping ranges
    const merged: [number, number][] = [ranges[0]];
    for (let i = 1; i < ranges.length; i++) {
      const [lo, hi] = ranges[i];
      const prev = merged[merged.length - 1];
      if (lo <= prev[1] + 1) {
        prev[1] = Math.max(prev[1], hi);
      } else {
        merged.push([lo, hi]);
      }
    }

    const rangeContent = new Map<string, string>();
    docExpanded.set(docId, rangeContent);

    for (const [lo, hi] of merged) {
      tasks.push(
        fetchContextChunks(docId, lo, hi).then((ctxChunks) => {
          const key = `${lo}:${hi}`;
          rangeContent.set(key, ctxChunks.map((c) => c.content).join("\n\n"));
        }),
      );
    }
  }

  return Promise.all(tasks).then(() => {
    // Rebuild results with expanded content
    const expanded: ChunkResult[] = [];
    for (const r of results) {
      const rangeContent = docExpanded.get(r.document_id)!;
      let found = false;
      for (const [key, content] of rangeContent) {
        const [lo, hi] = key.split(":").map(Number);
        if (lo <= r.chunk_index && r.chunk_index <= hi) {
          expanded.push({ ...r, content });
          found = true;
          break;
        }
      }
      if (!found) expanded.push(r);
    }

    // Deduplicate: keep only highest similarity per merged range
    const seen = new Set<string>();
    const deduped: ChunkResult[] = [];
    for (const r of expanded) {
      const rangeContent = docExpanded.get(r.document_id)!;
      for (const key of rangeContent.keys()) {
        const [lo, hi] = key.split(":").map(Number);
        if (lo <= r.chunk_index && r.chunk_index <= hi) {
          const dedupKey = `${r.document_id}:${lo}:${hi}`;
          if (!seen.has(dedupKey)) {
            seen.add(dedupKey);
            deduped.push(r);
          }
          break;
        }
      }
    }
    return deduped;
  });
}

export async function retrieve(
  query: string,
  topK: number = 5,
  tags?: string[],
): Promise<ChunkResult[]> {
  logger.debug({ query, topK, tags }, "retrieving chunks");
  const queryVec = await embedSingle(query);
  const results = await searchChunks(queryVec, topK, tags);
  logger.debug({ results: results.length }, "chunks retrieved");
  return expandResultsWithContext(results, CONTEXT_WINDOW);
}

export async function ask(
  query: string,
  topK: number = 5,
  tags?: string[],
): Promise<AskResponse> {
  const results = await retrieve(query, topK, tags);

  if (results.length === 0) {
    return { answer: "No relevant documents found.", sources: [] };
  }

  const contextBlock = results
    .map(
      (r) =>
        `[Source: ${r.document_name}, chunk ${r.chunk_index}, similarity ${r.similarity.toFixed(3)}]\n${r.content}`,
    )
    .join("\n\n---\n\n");

  const provider = createOllama({ baseURL: `${OLLAMA_BASE_URL}/api` });

  logger.info({ query, sources: results.length }, "generating LLM answer");
  const { text } = await generateText({
    model: provider.languageModel(CHAT_MODEL),
    system:
      "You are a helpful assistant. Answer the user's question using ONLY " +
      "the provided context. If the context doesn't contain enough information, " +
      "say so. Cite the source document names when possible.",
    prompt: `Context:\n${contextBlock}\n\nQuestion: ${query}`,
  });

  return {
    answer: text,
    sources: results.map((r) => ({
      document: r.document_name,
      chunk_index: r.chunk_index,
      similarity: r.similarity,
      excerpt:
        r.content.length > 200
          ? r.content.slice(0, 200) + "..."
          : r.content,
    })),
  };
}
