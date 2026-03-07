// -- Query result types --

export interface ChunkResult {
  chunk_id: number;
  document_id: number;
  chunk_index: number;
  content: string;
  similarity: number;
  document_name: string;
}

// -- Document listing types --

export interface Document {
  id: number;
  name: string;
  ingested_at: string;
  tags: Record<string, string>;
}

export interface Tag {
  key: string;
  value: string;
  doc_count: number;
}

export interface DocumentSummary {
  id: number;
  name: string;
  ingested_at: string;
}

// -- Ingest types --

export interface IngestResult {
  document_id: number;
  filename: string;
  characters: number;
  chunks: number;
  tags: Record<string, string>;
}

// -- Query/Ask types --

export interface AskSource {
  document: string;
  chunk_index: number;
  similarity: number;
  excerpt: string;
}

export interface AskResponse {
  answer: string;
  sources: AskSource[];
}

// -- API request types --

export interface IngestRequest {
  path?: string;
  tags?: Record<string, string>;
  extensions?: string[];
}

export interface QueryRequest {
  query?: string;
  top_k?: number;
  tags?: Record<string, string>;
}

export interface AgentRequest {
  message?: string;
  system_prompt?: string;
}

export interface FindDocumentsRequest {
  tags?: Record<string, string>;
}
