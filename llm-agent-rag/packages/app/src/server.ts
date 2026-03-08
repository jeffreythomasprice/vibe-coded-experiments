import Koa from "koa";
import Router from "@koa/router";
import { bodyParser } from "@koa/bodyparser";
import cors from "@koa/cors";
import multer from "@koa/multer";
import { readdir, unlink, rename } from "fs/promises";
import { join, extname, basename } from "path";
import { stat } from "fs/promises";
import { tmpdir } from "os";
import type { IngestRequest, QueryRequest, AgentRequest, FindDocumentsRequest, AddTagRequest } from "@rag/shared";
import {
  validateIngestRequest,
  validateQueryRequest,
  validateAgentRequest,
  validateFindDocumentsRequest,
  validateAddTagRequest,
} from "@rag/shared";
import logger from "./logger.js";
import { validateBody } from "./validate.js";

import { ingestFile } from "./ingest.js";
import { retrieve } from "./query.js";
import { agentChat } from "./agent.js";
import { listDocuments, listTags, searchTags, findDocumentsByTags, deleteDocument, deleteTag, insertTags, listConversations, getConversation, getConversationMessages, deleteConversation } from "./db.js";

const app = new Koa();
const router = new Router();

const upload = multer({ dest: join(tmpdir(), "rag-uploads") });

// Error middleware
app.use(async (ctx, next) => {
  try {
    await next();
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    const status = (err as { status?: number }).status ?? 500;
    ctx.status = status;
    ctx.body = { error: message };
    logger.error({ err, method: ctx.method, path: ctx.path, status }, "request error");
  }
});

// Request logging middleware
app.use(async (ctx, next) => {
  const start = Date.now();
  const reqId = crypto.randomUUID();
  ctx.state.reqId = reqId;
  await next();
  const duration = Date.now() - start;
  logger.info({ reqId, method: ctx.method, path: ctx.path, status: ctx.status, duration }, "request completed");
});

app.use(cors());
app.use(bodyParser());

// --- Routes ---

router.post("/api/ingest", validateBody(validateIngestRequest), async (ctx) => {
  const { path: filePath, tags, extensions } = ctx.request.body as IngestRequest;

  const stats = await stat(filePath);

  if (stats.isFile()) {
    if (extensions) {
      ctx.status = 400;
      ctx.body = { error: "extensions cannot be specified when path is a file" };
      return;
    }
    const result = await ingestFile(filePath, tags ?? []);
    ctx.body = result;
  } else if (stats.isDirectory()) {
    const exts = new Set(
      (extensions ?? [".txt", ".pdf"]).map((e) =>
        e.startsWith(".") ? e : `.${e}`,
      ),
    );

    async function findFiles(dir: string): Promise<string[]> {
      const entries = await readdir(dir, { withFileTypes: true });
      const files: string[] = [];
      for (const entry of entries) {
        const fullPath = join(dir, entry.name);
        if (entry.isDirectory()) {
          files.push(...(await findFiles(fullPath)));
        } else if (exts.has(extname(entry.name).toLowerCase())) {
          files.push(fullPath);
        }
      }
      return files;
    }

    const files = await findFiles(filePath);
    const results = [];
    for (const f of files) {
      const result = await ingestFile(f, tags ?? []);
      results.push(result);
    }
    ctx.body = { results };
  } else {
    ctx.status = 400;
    ctx.body = { error: "path is not a file or directory" };
  }
});

router.post("/api/ingest/upload", upload.single("file"), async (ctx) => {
  const file = ctx.file;
  if (!file) {
    ctx.status = 400;
    ctx.body = { error: "file is required" };
    return;
  }

  // Rename temp file to preserve original extension
  const ext = extname(file.originalname);
  const renamedPath = file.path + ext;
  await rename(file.path, renamedPath);

  try {
    let tags: string[] = [];
    if (ctx.request.body && typeof (ctx.request.body as Record<string, unknown>).tags === "string") {
      tags = JSON.parse((ctx.request.body as Record<string, string>).tags);
    }

    const result = await ingestFile(renamedPath, tags, file.originalname);
    ctx.body = result;
  } finally {
    await unlink(renamedPath).catch(() => {});
  }
});

router.post("/api/query", validateBody(validateQueryRequest), async (ctx) => {
  const { query, top_k, tags } = ctx.request.body as QueryRequest;

  const results = await retrieve(query, top_k ?? 5, tags);
  ctx.body = results;
});

router.post("/api/agent", validateBody(validateAgentRequest), async (ctx) => {
  const { message, conversation_id, system_prompt } = ctx.request.body as AgentRequest;

  const result = await agentChat(message, conversation_id, system_prompt);
  ctx.body = result;
});

router.get("/api/documents", async (ctx) => {
  const docs = await listDocuments();
  ctx.body = docs;
});

router.get("/api/tags", async (ctx) => {
  const tags = await listTags();
  ctx.body = tags;
});

router.get("/api/tags/search", async (ctx) => {
  const q = (ctx.query.q as string) ?? "";
  if (!q.trim()) {
    ctx.body = [];
    return;
  }
  const tags = await searchTags(q.trim());
  ctx.body = tags;
});

router.post("/api/documents/find", validateBody(validateFindDocumentsRequest), async (ctx) => {
  const { tags } = ctx.request.body as FindDocumentsRequest;

  const docs = await findDocumentsByTags(tags);
  ctx.body = docs;
});

router.delete("/api/documents/:id", async (ctx) => {
  const id = parseInt(ctx.params.id, 10);
  if (isNaN(id)) {
    ctx.status = 400;
    ctx.body = { error: "invalid document id" };
    return;
  }

  const deleted = await deleteDocument(id);
  if (!deleted) {
    ctx.status = 404;
    ctx.body = { error: "document not found" };
    return;
  }
  ctx.body = { ok: true };
});

router.post("/api/documents/:id/tags", validateBody(validateAddTagRequest), async (ctx) => {
  const id = parseInt(ctx.params.id, 10);
  if (isNaN(id)) {
    ctx.status = 400;
    ctx.body = { error: "invalid document id" };
    return;
  }
  const { tag } = ctx.request.body as AddTagRequest;
  await insertTags(id, [tag]);
  ctx.body = { ok: true };
});

router.delete("/api/documents/:id/tags/:tag", async (ctx) => {
  const id = parseInt(ctx.params.id, 10);
  if (isNaN(id)) {
    ctx.status = 400;
    ctx.body = { error: "invalid document id" };
    return;
  }

  await deleteTag(id, decodeURIComponent(ctx.params.tag));
  ctx.body = { ok: true };
});

router.get("/api/conversations", async (ctx) => {
  const conversations = await listConversations();
  ctx.body = conversations;
});

router.get("/api/conversations/:id", async (ctx) => {
  const id = parseInt(ctx.params.id, 10);
  if (isNaN(id)) {
    ctx.status = 400;
    ctx.body = { error: "invalid conversation id" };
    return;
  }
  const conversation = await getConversation(id);
  if (!conversation) {
    ctx.status = 404;
    ctx.body = { error: "conversation not found" };
    return;
  }
  const messages = await getConversationMessages(id);
  ctx.body = { ...conversation, messages };
});

router.delete("/api/conversations/:id", async (ctx) => {
  const id = parseInt(ctx.params.id, 10);
  if (isNaN(id)) {
    ctx.status = 400;
    ctx.body = { error: "invalid conversation id" };
    return;
  }
  const deleted = await deleteConversation(id);
  if (!deleted) {
    ctx.status = 404;
    ctx.body = { error: "conversation not found" };
    return;
  }
  ctx.body = { ok: true };
});

app.use(router.routes());
app.use(router.allowedMethods());

export async function startServer(): Promise<void> {
  const port = parseInt(Bun.env.PORT ?? "8001", 10);
  const host = Bun.env.BIND_ADDRESS ?? "127.0.0.1";

  logger.info({ host, port }, "starting server");
  app.listen(port, host, () => {
    logger.info({ host, port }, "RAG API server started successfully");
  });
}
