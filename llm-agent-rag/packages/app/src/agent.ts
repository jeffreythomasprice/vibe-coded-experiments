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
    logger.info({ query, top_k, tags }, "agent tool call: search_documents");
    const results = await retrieve(query, top_k, tags);
    logger.info({ resultCount: results.length }, "agent tool result: search_documents");
    logger.debug({ results }, "agent tool result details: search_documents");
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
): Promise<{ conversation_id: number; messages: ConversationMessage[] }> {
  const system = systemPrompt ?? DEFAULT_SYSTEM;
  logger.info({ userMessage, conversationId, systemPrompt: system }, "agent chat started");

  // Create or reuse conversation
  let convId: number;
  if (conversationId) {
    convId = conversationId;
    logger.info({ convId }, "reusing existing conversation");
  } else {
    const title = userMessage.slice(0, 100);
    convId = await createConversation(title);
    logger.info({ convId, title }, "created new conversation");
  }

  // Persist user message
  const userMsg = await insertConversationMessage(convId, "user", userMessage);
  const newMessages: ConversationMessage[] = [userMsg];

  // Load history and build LLM messages (only user + assistant for the LLM)
  const allDbMessages = await getConversationMessages(convId);
  logger.info({ convId, historyCount: allDbMessages.length }, "loaded conversation history");

  const llmMessages: Array<{ role: "user" | "assistant"; content: string }> = [];
  for (const m of allDbMessages) {
    if (m.role === "user" || m.role === "assistant") {
      llmMessages.push({ role: m.role, content: m.content });
    }
  }

  logger.info({ llmMessageCount: llmMessages.length, model: CHAT_MODEL }, "sending messages to LLM");
  logger.debug({ llmMessages }, "LLM input messages");

  const provider = createOllama({ baseURL: `${OLLAMA_BASE_URL}/api` });

  const { text, steps, usage, finishReason } = await generateText({
    model: provider.languageModel(CHAT_MODEL),
    system,
    messages: llmMessages,
    tools: { search_documents: searchDocuments },
    stopWhen: stepCountIs(10),
  });

  logger.info(
    { stepCount: steps.length, finishReason, textLength: text?.length ?? 0, usage },
    "LLM generateText completed",
  );

  // Extract tool calls/results from steps and persist them
  for (let stepIdx = 0; stepIdx < steps.length; stepIdx++) {
    const step = steps[stepIdx];
    const toolCalls = step.toolCalls ?? [];
    const toolResults = step.toolResults ?? [];

    logger.info(
      { stepIdx, toolCallCount: toolCalls.length, hasText: !!step.text, finishReason: step.finishReason },
      "processing agent step",
    );
    if (step.text) {
      logger.debug({ stepIdx, text: step.text }, "step text output");
    }

    for (let i = 0; i < toolCalls.length; i++) {
      const tc = toolCalls[i] as { toolName: string; input: unknown };
      const input = tc.input as Record<string, unknown> | undefined;
      logger.info({ stepIdx, toolName: tc.toolName, input }, "persisting tool call");
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
        logger.info({ tr }, "TODO JEFF wut?");
        const output = tr.output;
        const resultCount = Array.isArray(output) ? output.length : undefined;
        let content: string;
        if (Array.isArray(output) && output.length > 0 && output[0]?.document_name) {
          const lines = [`Found ${output.length} results`];
          const sorted = [...output].sort((a, b) => {
            const nameA = a.document_name ?? "Unknown";
            const nameB = b.document_name ?? "Unknown";
            if (nameA !== nameB) return nameA.localeCompare(nameB);
            return (a.page_start ?? 0) - (b.page_start ?? 0);
          });
          for (const chunk of sorted) {
            const name = chunk.document_name ?? "Unknown";
            const pageStart = chunk.page_start ?? null;
            const pageEnd = chunk.page_end ?? null;
            if (pageStart != null) {
              const range = pageEnd != null && pageEnd !== pageStart
                ? `pp. ${pageStart}-${pageEnd}`
                : `p. ${pageStart}`;
              lines.push(`- ${name} ${range}`);
            } else {
              lines.push(`- ${name}`);
            }
          }
          content = lines.join("\n");
        } else if (resultCount !== undefined) {
          content = `Found ${resultCount} results`;
        } else {
          const resultContent = typeof output === "string" ? output : JSON.stringify(output);
          content = resultContent.slice(0, 200);
        }
        const toolResultMsg = await insertConversationMessage(
          convId,
          "tool_result",
          content,
          { name: tc.toolName, result_count: resultCount },
        );
        newMessages.push(toolResultMsg);
      }
    }
  }

  // Persist final assistant text
  const finalText = text || "Agent reached maximum iterations without a final answer.";
  logger.info({ convId, finalTextLength: finalText.length }, "persisting final assistant message");
  logger.debug({ finalText }, "final assistant text");
  const assistantMsg = await insertConversationMessage(convId, "assistant", finalText);
  newMessages.push(assistantMsg);

  await updateConversationTimestamp(convId);

  logger.info({ convId, newMessageCount: newMessages.length }, "agent chat completed");
  return { conversation_id: convId, messages: newMessages };
}
