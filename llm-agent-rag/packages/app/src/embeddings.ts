import { embedMany, embed } from "ai";
import logger from "./logger.js";
import { EMBED_MODEL } from "./config.js";
import { getEmbeddingModel } from "./providers.js";

export async function embedTexts(texts: string[]): Promise<number[][]> {
  logger.debug({ count: texts.length, model: EMBED_MODEL }, "embedding batch");
  const model = getEmbeddingModel();
  const { embeddings } = await embedMany({ model, values: texts });
  return embeddings;
}

export async function embedSingle(text: string): Promise<number[]> {
  const model = getEmbeddingModel();
  const { embedding } = await embed({ model, value: text });
  return embedding;
}
