import postgres from "postgres";
import type { ChunkResult, Document, Tag, DocumentSummary, ConversationMessage, Conversation } from "@rag/shared";
import logger from "./logger.js";
import { DB_DSN, EMBED_DIM, EMBED_PROVIDER, EMBED_MODEL } from "./config.js";

export type { ChunkResult, Document, Tag, DocumentSummary, ConversationMessage, Conversation };

let sql: ReturnType<typeof postgres>;

function getSql() {
  if (!sql) {
    sql = postgres(DB_DSN);
  }
  return sql;
}

export async function closeSql(): Promise<void> {
  if (sql) await sql.end();
}

function chunksTable(): string {
  return `chunks_${EMBED_DIM}`;
}

// -- Cache helpers --

export async function getCachedValue(key: string): Promise<string | null> {
  const s = getSql();
  const rows = await s`SELECT value FROM cache WHERE key = ${key}`;
  return rows.length > 0 ? (rows[0].value as string) : null;
}

export async function setCachedValue(key: string, value: string): Promise<void> {
  const s = getSql();
  await s`
    INSERT INTO cache (key, value) VALUES (${key}, ${value})
    ON CONFLICT (key) DO UPDATE SET value = ${value}
  `;
}

// -- Dynamic table creation --

export async function ensureChunksTable(dim: number): Promise<void> {
  logger.debug({ dim }, "ensuring chunks table exists");
  const s = getSql();
  const table = `chunks_${dim}`;
  await s.unsafe(`
    CREATE TABLE IF NOT EXISTS ${table} (
        id              SERIAL PRIMARY KEY,
        document_id     INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
        chunk_index     INTEGER NOT NULL,
        content         TEXT NOT NULL,
        embed_provider  TEXT NOT NULL,
        embed_model     TEXT NOT NULL,
        embedding       vector(${dim}),
        page_start      INTEGER,
        page_end        INTEGER
    )
  `);
  await s.unsafe(`
    CREATE INDEX IF NOT EXISTS idx_${table}_doc ON ${table} (document_id)
  `);
  await s.unsafe(`
    CREATE INDEX IF NOT EXISTS idx_${table}_embedding ON ${table} USING hnsw (embedding vector_cosine_ops)
  `);
}

// -- Ingest helpers --

export async function insertDocument(name: string): Promise<number> {
  logger.debug({ name }, "inserting document");
  const s = getSql();
  const [row] = await s`INSERT INTO documents (name) VALUES (${name}) RETURNING id`;
  return row.id as number;
}

export async function deleteTag(
  documentId: number,
  tag: string,
): Promise<void> {
  const s = getSql();
  await s`DELETE FROM document_tags WHERE document_id = ${documentId} AND tag = ${tag}`;
}

export async function insertTags(
  documentId: number,
  tags: string[],
): Promise<void> {
  if (tags.length === 0) return;
  const s = getSql();
  const values = tags.map((tag) => ({
    document_id: documentId,
    tag,
  }));
  await s`INSERT INTO document_tags ${s(values, "document_id", "tag")} ON CONFLICT DO NOTHING`;
}

export async function insertChunks(
  documentId: number,
  chunks: { chunkIndex: number; content: string; embedding: number[]; pageStart?: number | null; pageEnd?: number | null }[],
): Promise<void> {
  const s = getSql();
  const table = chunksTable();
  logger.debug({ documentId, chunks: chunks.length, table }, "inserting chunks");
  for (const chunk of chunks) {
    const vecStr = `[${chunk.embedding.join(",")}]`;
    await s.unsafe(
      `INSERT INTO ${table} (document_id, chunk_index, content, embed_provider, embed_model, embedding, page_start, page_end)
       VALUES ($1, $2, $3, $4, $5, $6::vector, $7, $8)`,
      [documentId, chunk.chunkIndex, chunk.content, EMBED_PROVIDER, EMBED_MODEL, vecStr, chunk.pageStart ?? null, chunk.pageEnd ?? null],
    );
  }
}

// -- Query helpers --

export async function searchChunks(
  queryEmbedding: number[],
  topK: number = 5,
  tags?: string[],
): Promise<ChunkResult[]> {
  logger.debug({ topK, tags }, "searching chunks");
  const s = getSql();
  const table = chunksTable();
  const vecStr = `[${queryEmbedding.join(",")}]`;

  let tagClauses = "";
  const params: (string | number)[] = [vecStr, EMBED_PROVIDER, EMBED_MODEL, topK];
  let paramIdx = 5;

  if (tags && tags.length > 0) {
    const parts: string[] = [];
    for (const tag of tags) {
      parts.push(
        `EXISTS (SELECT 1 FROM document_tags t WHERE t.document_id = c.document_id AND t.tag = $${paramIdx})`,
      );
      params.push(tag);
      paramIdx += 1;
    }
    tagClauses = "AND " + parts.join(" AND ");
  }

  const rows = await s.unsafe(
    `SELECT c.id, c.document_id, c.chunk_index, c.content,
            1 - (c.embedding <=> $1::vector) AS similarity,
            d.name AS document_name,
            c.page_start, c.page_end
     FROM ${table} c
     JOIN documents d ON d.id = c.document_id
     WHERE c.embed_provider = $2 AND c.embed_model = $3
     ${tagClauses}
     ORDER BY c.embedding <=> $1::vector
     LIMIT $4`,
    params,
  );

  return rows.map((r) => ({
    chunk_id: r.id as number,
    document_id: r.document_id as number,
    chunk_index: r.chunk_index as number,
    content: r.content as string,
    similarity: parseFloat(r.similarity as string),
    document_name: r.document_name as string,
    page_start: r.page_start != null ? (r.page_start as number) : null,
    page_end: r.page_end != null ? (r.page_end as number) : null,
  }));
}

export async function fetchContextChunks(
  documentId: number,
  indexStart: number,
  indexEnd: number,
): Promise<{ chunk_index: number; content: string }[]> {
  const s = getSql();
  const table = chunksTable();
  const rows = await s.unsafe(
    `SELECT chunk_index, content FROM ${table}
     WHERE document_id = $1 AND embed_provider = $2 AND embed_model = $3
       AND chunk_index BETWEEN $4 AND $5
     ORDER BY chunk_index`,
    [documentId, EMBED_PROVIDER, EMBED_MODEL, indexStart, indexEnd],
  );
  return rows.map((r) => ({
    chunk_index: r.chunk_index as number,
    content: r.content as string,
  }));
}

// -- Listing helpers --

export async function listDocuments(): Promise<Document[]> {
  const s = getSql();
  const rows = await s`
    SELECT d.id, d.name, d.ingested_at, dt.tag
    FROM documents d
    LEFT JOIN document_tags dt ON dt.document_id = d.id
    ORDER BY d.name, d.ingested_at
  `;

  const docs = new Map<number, Document>();
  for (const r of rows) {
    const docId = r.id as number;
    if (!docs.has(docId)) {
      docs.set(docId, {
        id: docId,
        name: r.name as string,
        ingested_at: String(r.ingested_at),
        tags: [],
      });
    }
    if (r.tag != null) {
      docs.get(docId)!.tags.push(r.tag as string);
    }
  }
  return Array.from(docs.values());
}

export async function listTags(): Promise<Tag[]> {
  const s = getSql();
  const rows = await s`
    SELECT tag, COUNT(*) AS doc_count
    FROM document_tags
    GROUP BY tag
    ORDER BY tag
  `;
  return rows.map((r) => ({
    tag: r.tag as string,
    doc_count: Number(r.doc_count),
  }));
}

export async function searchTags(query: string): Promise<Tag[]> {
  const s = getSql();
  const pattern = `%${query}%`;
  const rows = await s`
    SELECT tag, COUNT(*) AS doc_count
    FROM document_tags
    WHERE tag ILIKE ${pattern}
    GROUP BY tag
    ORDER BY tag
  `;
  return rows.map((r) => ({
    tag: r.tag as string,
    doc_count: Number(r.doc_count),
  }));
}

export async function deleteDocument(id: number): Promise<boolean> {
  const s = getSql();
  const rows = await s`DELETE FROM documents WHERE id = ${id} RETURNING id`;
  return rows.length > 0;
}

// -- Conversation helpers --

export async function createConversation(title: string | null): Promise<number> {
  const s = getSql();
  const [row] = await s`INSERT INTO conversations (title) VALUES (${title}) RETURNING id`;
  return row.id as number;
}

export async function getConversation(id: number): Promise<Conversation | null> {
  const s = getSql();
  const rows = await s`SELECT id, title, created_at, updated_at FROM conversations WHERE id = ${id}`;
  if (rows.length === 0) return null;
  const r = rows[0];
  return { id: r.id as number, title: r.title as string | null, created_at: String(r.created_at), updated_at: String(r.updated_at) };
}

export async function getConversationMessages(conversationId: number): Promise<ConversationMessage[]> {
  const s = getSql();
  const rows = await s`
    SELECT id, role, content, tool_info, created_at
    FROM conversation_messages
    WHERE conversation_id = ${conversationId}
    ORDER BY created_at, id
  `;
  return rows.map((r) => ({
    id: r.id as number,
    role: r.role as string,
    content: r.content as string,
    tool_info: r.tool_info ?? undefined,
    created_at: String(r.created_at),
  }));
}

export async function insertConversationMessage(
  conversationId: number,
  role: string,
  content: string,
  toolInfo?: unknown,
): Promise<ConversationMessage> {
  const s = getSql();
  const [row] = await s`
    INSERT INTO conversation_messages (conversation_id, role, content, tool_info)
    VALUES (${conversationId}, ${role}, ${content}, ${toolInfo ? s.json(toolInfo as never) : null})
    RETURNING id, role, content, tool_info, created_at
  `;
  return {
    id: row.id as number,
    role: row.role as string,
    content: row.content as string,
    tool_info: row.tool_info ?? undefined,
    created_at: String(row.created_at),
  };
}

export async function updateConversationTimestamp(id: number): Promise<void> {
  const s = getSql();
  await s`UPDATE conversations SET updated_at = now() WHERE id = ${id}`;
}

export async function listConversations(): Promise<Conversation[]> {
  const s = getSql();
  const rows = await s`SELECT id, title, created_at, updated_at FROM conversations ORDER BY updated_at DESC`;
  return rows.map((r) => ({
    id: r.id as number,
    title: r.title as string | null,
    created_at: String(r.created_at),
    updated_at: String(r.updated_at),
  }));
}

export async function deleteConversation(id: number): Promise<boolean> {
  const s = getSql();
  const rows = await s`DELETE FROM conversations WHERE id = ${id} RETURNING id`;
  return rows.length > 0;
}

export async function findDocumentsByTags(
  tags: string[],
): Promise<DocumentSummary[]> {
  const s = getSql();
  const clauses = tags.map(
    (tag) =>
      s`EXISTS (SELECT 1 FROM document_tags t WHERE t.document_id = d.id AND t.tag = ${tag})`,
  );
  const where = clauses.reduce((a, b) => s`${a} AND ${b}`);

  const rows = await s`
    SELECT d.id, d.name, d.ingested_at
    FROM documents d
    WHERE ${where}
    ORDER BY d.name, d.ingested_at
  `;
  return rows.map((r) => ({
    id: r.id as number,
    name: r.name as string,
    ingested_at: String(r.ingested_at),
  }));
}
