import type { LanguageModel, EmbeddingModel } from "ai";
import { createOllama } from "ollama-ai-provider-v2";
import { createAnthropic } from "@ai-sdk/anthropic";
import { CHAT_PROVIDER, CHAT_MODEL, EMBED_PROVIDER, EMBED_MODEL, OLLAMA_BASE_URL } from "./config.js";

const SUPPORTED_PROVIDERS = ["ollama", "anthropic"] as const;

let ollamaInstance: ReturnType<typeof createOllama> | undefined;
let anthropicInstance: ReturnType<typeof createAnthropic> | undefined;

function getOllama() {
  if (!ollamaInstance) {
    ollamaInstance = createOllama({ baseURL: `${OLLAMA_BASE_URL}/api` });
  }
  return ollamaInstance;
}

function getAnthropic() {
  if (!anthropicInstance) {
    anthropicInstance = createAnthropic();
  }
  return anthropicInstance;
}

export function getChatModel(): LanguageModel {
  switch (CHAT_PROVIDER) {
    case "ollama":
      return getOllama().languageModel(CHAT_MODEL);
    case "anthropic":
      return getAnthropic().languageModel(CHAT_MODEL);
    default:
      throw new Error(
        `Unsupported provider: "${CHAT_PROVIDER}". Supported: ${SUPPORTED_PROVIDERS.join(", ")}`,
      );
  }
}

export function getEmbeddingModel(): EmbeddingModel {
  switch (EMBED_PROVIDER) {
    case "ollama":
      return getOllama().textEmbeddingModel(EMBED_MODEL);
    case "anthropic":
      throw new Error("Anthropic does not support embeddings. Use ollama for EMBED_PROVIDER.");
    default:
      throw new Error(
        `Unsupported provider: "${EMBED_PROVIDER}". Supported: ${SUPPORTED_PROVIDERS.join(", ")}`,
      );
  }
}
