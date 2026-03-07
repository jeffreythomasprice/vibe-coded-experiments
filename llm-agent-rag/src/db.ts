import postgres from "postgres";
import { DB_DSN } from "./config.js";

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

// -- Ingest helpers --

export async function insertDocument(name: string): Promise<number> {
  const s = getSql();
  const [row] = await s`INSERT INTO documents (name) VALUES (${name}) RETURNING id`;
  return row.id as number;
}

export async function insertTags(
  documentId: number,
  tags: Record<string, string>,
): Promise<void> {
  const entries = Object.entries(tags);
  if (entries.length === 0) return;
  const s = getSql();
  const values = entries.map(([key, value]) => ({
    document_id: documentId,
    key,
    value,
  }));
  await s`INSERT INTO document_tags ${s(values, "document_id", "key", "value")}`;
}

export async function insertChunks(
  documentId: number,
  chunks: { chunkIndex: number; content: string; embedding: number[] }[],
): Promise<void> {
  const s = getSql();
  for (const chunk of chunks) {
    const vecStr = `[${chunk.embedding.join(",")}]`;
    await s`
      INSERT INTO chunks (document_id, chunk_index, content, embedding)
      VALUES (${documentId}, ${chunk.chunkIndex}, ${chunk.content}, ${vecStr}::vector)
    `;
  }
}

// -- Query helpers --

export interface ChunkResult {
  chunk_id: number;
  document_id: number;
  chunk_index: number;
  content: string;
  similarity: number;
  document_name: string;
}

export async function searchChunks(
  queryEmbedding: number[],
  topK: number = 5,
  tags?: Record<string, string>,
): Promise<ChunkResult[]> {
  const s = getSql();
  const vecStr = `[${queryEmbedding.join(",")}]`;

  let tagFilter = s``;
  if (tags && Object.keys(tags).length > 0) {
    const clauses = Object.entries(tags).map(
      ([key, value]) =>
        s`EXISTS (SELECT 1 FROM document_tags t WHERE t.document_id = c.document_id AND t.key = ${key} AND t.value = ${value})`,
    );
    tagFilter = s`WHERE ${clauses.reduce((a, b) => s`${a} AND ${b}`)}`;
  }

  const rows = await s`
    SELECT c.id, c.document_id, c.chunk_index, c.content,
           1 - (c.embedding <=> ${vecStr}::vector) AS similarity,
           d.name AS document_name
    FROM chunks c
    JOIN documents d ON d.id = c.document_id
    ${tagFilter}
    ORDER BY c.embedding <=> ${vecStr}::vector
    LIMIT ${topK}
  `;

  return rows.map((r) => ({
    chunk_id: r.id as number,
    document_id: r.document_id as number,
    chunk_index: r.chunk_index as number,
    content: r.content as string,
    similarity: parseFloat(r.similarity as string),
    document_name: r.document_name as string,
  }));
}

export async function fetchContextChunks(
  documentId: number,
  indexStart: number,
  indexEnd: number,
): Promise<{ chunk_index: number; content: string }[]> {
  const s = getSql();
  const rows = await s`
    SELECT chunk_index, content FROM chunks
    WHERE document_id = ${documentId} AND chunk_index BETWEEN ${indexStart} AND ${indexEnd}
    ORDER BY chunk_index
  `;
  return rows.map((r) => ({
    chunk_index: r.chunk_index as number,
    content: r.content as string,
  }));
}

// -- Listing helpers --

export async function listDocuments(): Promise<
  { id: number; name: string; ingested_at: string; tags: Record<string, string> }[]
> {
  const s = getSql();
  const rows = await s`
    SELECT d.id, d.name, d.ingested_at, dt.key, dt.value
    FROM documents d
    LEFT JOIN document_tags dt ON dt.document_id = d.id
    ORDER BY d.id
  `;

  const docs = new Map<
    number,
    { id: number; name: string; ingested_at: string; tags: Record<string, string> }
  >();
  for (const r of rows) {
    const docId = r.id as number;
    if (!docs.has(docId)) {
      docs.set(docId, {
        id: docId,
        name: r.name as string,
        ingested_at: String(r.ingested_at),
        tags: {},
      });
    }
    if (r.key != null) {
      docs.get(docId)!.tags[r.key as string] = r.value as string;
    }
  }
  return Array.from(docs.values());
}

export async function listTags(): Promise<
  { key: string; value: string; doc_count: number }[]
> {
  const s = getSql();
  const rows = await s`
    SELECT key, value, COUNT(*) AS doc_count
    FROM document_tags
    GROUP BY key, value
    ORDER BY key, value
  `;
  return rows.map((r) => ({
    key: r.key as string,
    value: r.value as string,
    doc_count: Number(r.doc_count),
  }));
}

export async function findDocumentsByTags(
  tags: Record<string, string>,
): Promise<{ id: number; name: string; ingested_at: string }[]> {
  const s = getSql();
  const clauses = Object.entries(tags).map(
    ([key, value]) =>
      s`EXISTS (SELECT 1 FROM document_tags t WHERE t.document_id = d.id AND t.key = ${key} AND t.value = ${value})`,
  );
  const where = clauses.reduce((a, b) => s`${a} AND ${b}`);

  const rows = await s`
    SELECT d.id, d.name, d.ingested_at
    FROM documents d
    WHERE ${where}
    ORDER BY d.name
  `;
  return rows.map((r) => ({
    id: r.id as number,
    name: r.name as string,
    ingested_at: String(r.ingested_at),
  }));
}
