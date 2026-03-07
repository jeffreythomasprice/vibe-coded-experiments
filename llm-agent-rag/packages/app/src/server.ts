import Koa from "koa";
import Router from "@koa/router";
import { bodyParser } from "@koa/bodyparser";
import cors from "@koa/cors";
import multer from "@koa/multer";
import { readdir, unlink, rename } from "fs/promises";
import { join, extname, basename } from "path";
import { stat } from "fs/promises";
import { tmpdir } from "os";
import type { IngestRequest, QueryRequest, AgentRequest, FindDocumentsRequest } from "@rag/shared";

import { ingestFile } from "./ingest.js";
import { retrieve, ask } from "./query.js";
import { agentChat } from "./agent.js";
import { listDocuments, listTags, findDocumentsByTags, deleteDocument } from "./db.js";

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
  }
});

app.use(cors());
app.use(bodyParser());

// --- Routes ---

router.post("/api/ingest", async (ctx) => {
  const { path: filePath, tags, extensions } = ctx.request.body as IngestRequest;

  if (!filePath) {
    ctx.status = 400;
    ctx.body = { error: "path is required" };
    return;
  }

  const stats = await stat(filePath);

  if (stats.isFile()) {
    if (extensions) {
      ctx.status = 400;
      ctx.body = { error: "extensions cannot be specified when path is a file" };
      return;
    }
    const result = await ingestFile(filePath, tags ?? {});
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
      const perFileTags = { ...(tags ?? {}), filename: basename(f) };
      const result = await ingestFile(f, perFileTags);
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
    let tags: Record<string, string> = {};
    if (ctx.request.body && typeof (ctx.request.body as Record<string, unknown>).tags === "string") {
      tags = JSON.parse((ctx.request.body as Record<string, string>).tags);
    }

    const result = await ingestFile(renamedPath, tags, file.originalname);
    ctx.body = result;
  } finally {
    await unlink(renamedPath).catch(() => {});
  }
});

router.post("/api/query", async (ctx) => {
  const { query, top_k, tags } = ctx.request.body as QueryRequest;

  if (!query) {
    ctx.status = 400;
    ctx.body = { error: "query is required" };
    return;
  }

  const results = await retrieve(query, top_k ?? 5, tags);
  ctx.body = results;
});

router.post("/api/ask", async (ctx) => {
  const { query, top_k, tags } = ctx.request.body as QueryRequest;

  if (!query) {
    ctx.status = 400;
    ctx.body = { error: "query is required" };
    return;
  }

  const result = await ask(query, top_k ?? 5, tags);
  ctx.body = result;
});

router.post("/api/agent", async (ctx) => {
  const { message, system_prompt } = ctx.request.body as AgentRequest;

  if (!message) {
    ctx.status = 400;
    ctx.body = { error: "message is required" };
    return;
  }

  const answer = await agentChat(message, system_prompt);
  ctx.body = { answer };
});

router.get("/api/documents", async (ctx) => {
  const docs = await listDocuments();
  ctx.body = docs;
});

router.get("/api/tags", async (ctx) => {
  const tags = await listTags();
  ctx.body = tags;
});

router.post("/api/documents/find", async (ctx) => {
  const { tags } = ctx.request.body as FindDocumentsRequest;

  if (!tags || Object.keys(tags).length === 0) {
    ctx.status = 400;
    ctx.body = { error: "tags is required and must not be empty" };
    return;
  }

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

app.use(router.routes());
app.use(router.allowedMethods());

export async function startServer(): Promise<void> {
  const port = parseInt(Bun.env.PORT ?? "8001", 10);
  const host = Bun.env.BIND_ADDRESS ?? "127.0.0.1";

  console.log(`Starting server on ${host}:${port}...`);
  app.listen(port, host, () => {
    console.log(`RAG API server started successfully on http://${host}:${port}`);
  });
}
