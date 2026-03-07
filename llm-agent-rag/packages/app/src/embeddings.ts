import { createOllama } from "ollama-ai-provider-v2";
import { embedMany, embed } from "ai";
import { EMBED_MODEL, OLLAMA_BASE_URL } from "./config.js";

function getProvider() {
  return createOllama({ baseURL: `${OLLAMA_BASE_URL}/api` });
}

export async function embedTexts(texts: string[]): Promise<number[][]> {
  const model = getProvider().textEmbeddingModel(EMBED_MODEL);
  const { embeddings } = await embedMany({ model, values: texts });
  return embeddings;
}

export async function embedSingle(text: string): Promise<number[]> {
  const model = getProvider().textEmbeddingModel(EMBED_MODEL);
  const { embedding } = await embed({ model, value: text });
  return embedding;
}
