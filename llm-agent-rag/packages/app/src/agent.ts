import { createOllama } from "ollama-ai-provider-v2";
import { generateText, tool, stepCountIs } from "ai";
import { z } from "zod";
import type { ConversationMessage } from "@rag/shared";
import logger from "./logger.js";
import { CHAT_MODEL, OLLAMA_BASE_URL } from "./config.js";
import { retrieve } from "./query.js";
import {
  createConversation,
  getConversationMessages,
  insertConversationMessage,
  updateConversationTimestamp,
} from "./db.js";

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
      .array(z.string())
      .optional()
      .describe(
        'Optional tag filters. Example: ["project=alpha", "filename=report.pdf"]',
      ),
  }),
  execute: async ({ query, top_k, tags }) => {
    logger.debug({ query, top_k, tags }, "agent tool call: search_documents");
    const results = await retrieve(query, top_k, tags);
    return results;
  },
});

const DEFAULT_SYSTEM =
  "You are a helpful research assistant with access to a local document " +
  "store. Use the search_documents tool to find relevant information " +
  "before answering. You may call the tool multiple times with different " +
  "queries or tag filters. Cite source documents when possible.";

export async function agentChat(
  userMessage: string,
  conversationId?: number | null,
  systemPrompt?: string,
): Promise<{ conversationId: number; messages: ConversationMessage[] }> {
  const system = systemPrompt ?? DEFAULT_SYSTEM;
  logger.info({ userMessage, conversationId }, "agent chat started");

  // Create or reuse conversation
  let convId: number;
  if (conversationId) {
    convId = conversationId;
  } else {
    const title = userMessage.slice(0, 100);
    convId = await createConversation(title);
  }

  // Persist user message
  const userMsg = await insertConversationMessage(convId, "user", userMessage);
  const newMessages: ConversationMessage[] = [userMsg];

  // Load history and build LLM messages (only user + assistant for the LLM)
  const allDbMessages = await getConversationMessages(convId);
  const llmMessages: Array<{ role: "user" | "assistant"; content: string }> = [];
  for (const m of allDbMessages) {
    if (m.role === "user" || m.role === "assistant") {
      llmMessages.push({ role: m.role, content: m.content });
    }
  }

  const provider = createOllama({ baseURL: `${OLLAMA_BASE_URL}/api` });

  const { text, steps } = await generateText({
    model: provider.languageModel(CHAT_MODEL),
    system,
    messages: llmMessages,
    tools: { search_documents: searchDocuments },
    stopWhen: stepCountIs(10),
  });

  // Extract tool calls/results from steps and persist them
  for (const step of steps) {
    const toolCalls = step.toolCalls ?? [];
    const toolResults = step.toolResults ?? [];

    for (let i = 0; i < toolCalls.length; i++) {
      const tc = toolCalls[i] as { toolName: string; input: unknown };
      const input = tc.input as Record<string, unknown> | undefined;
      const toolCallMsg = await insertConversationMessage(
        convId,
        "tool_call",
        `Tool: ${tc.toolName}`,
        { name: tc.toolName, args: input },
      );
      newMessages.push(toolCallMsg);

      // Match tool result if available
      const tr = toolResults[i] as { output: unknown } | undefined;
      if (tr) {
        const output = tr.output;
        const resultContent =
          typeof output === "string"
            ? output
            : JSON.stringify(output);
        const resultCount = Array.isArray(output) ? output.length : undefined;
        const toolResultMsg = await insertConversationMessage(
          convId,
          "tool_result",
          resultCount !== undefined
            ? `Found ${resultCount} results`
            : resultContent.slice(0, 200),
          { name: tc.toolName, result_count: resultCount },
        );
        newMessages.push(toolResultMsg);
      }
    }
  }

  // Persist final assistant text
  const finalText = text || "Agent reached maximum iterations without a final answer.";
  const assistantMsg = await insertConversationMessage(convId, "assistant", finalText);
  newMessages.push(assistantMsg);

  await updateConversationTimestamp(convId);

  return { conversationId: convId, messages: newMessages };
}
