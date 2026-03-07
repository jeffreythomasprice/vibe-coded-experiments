import { createOllama } from "ollama-ai-provider-v2";
import { generateText, tool, stepCountIs } from "ai";
import { z } from "zod";
import { CHAT_MODEL, OLLAMA_BASE_URL } from "./config.js";
import { retrieve } from "./query.js";

const searchDocuments = tool({
  description:
    "Search the local document store using semantic similarity. " +
    "Use this to find information from ingested documents.",
  inputSchema: z.object({
    query: z.string().describe("Natural-language search query."),
    top_k: z
      .number()
      .default(5)
      .describe("Number of results to return (default 5)."),
    tags: z
      .record(z.string(), z.string())
      .optional()
      .describe(
        'Optional key=value tag filters. Example: {"project": "alpha", "filename": "report.pdf"}',
      ),
  }),
  execute: async ({ query, top_k, tags }) => {
    const results = await retrieve(query, top_k, tags);
    return results;
  },
});

export async function agentChat(
  userMessage: string,
  systemPrompt?: string,
): Promise<string> {
  const system =
    systemPrompt ??
    "You are a helpful research assistant with access to a local document " +
      "store. Use the search_documents tool to find relevant information " +
      "before answering. You may call the tool multiple times with different " +
      "queries or tag filters. Cite source documents when possible.";

  const provider = createOllama({ baseURL: `${OLLAMA_BASE_URL}/api` });

  const { text } = await generateText({
    model: provider.languageModel(CHAT_MODEL),
    system,
    prompt: userMessage,
    tools: { search_documents: searchDocuments },
    stopWhen: stepCountIs(10),
  });

  return text || "Agent reached maximum iterations without a final answer.";
}
