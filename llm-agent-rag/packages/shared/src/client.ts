import type {
  Document,
  Tag,
  DocumentSummary,
  IngestResult,
  ChunkResult,
  AskResponse,
} from "./generated/types.js";

export class RagClient {
  constructor(private baseUrl: string) {
    // Strip trailing slash
    if (this.baseUrl.endsWith("/")) {
      this.baseUrl = this.baseUrl.slice(0, -1);
    }
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await fetch(`${this.baseUrl}${path}`, init);
    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
    return res.json() as Promise<T>;
  }

  private jsonPost<T>(path: string, body: unknown): Promise<T> {
    return this.request<T>(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  }

  async listDocuments(): Promise<Document[]> {
    return this.request<Document[]>("/api/documents");
  }

  async deleteDocument(id: number): Promise<{ ok: boolean }> {
    return this.request<{ ok: boolean }>(`/api/documents/${id}`, {
      method: "DELETE",
    });
  }

  async addTag(documentId: number, tag: string): Promise<{ ok: boolean }> {
    return this.jsonPost<{ ok: boolean }>(`/api/documents/${documentId}/tags`, { tag });
  }

  async deleteTag(documentId: number, tag: string): Promise<{ ok: boolean }> {
    return this.request<{ ok: boolean }>(`/api/documents/${documentId}/tags/${encodeURIComponent(tag)}`, {
      method: "DELETE",
    });
  }

  async listTags(): Promise<Tag[]> {
    return this.request<Tag[]>("/api/tags");
  }

  async searchTags(query: string): Promise<Tag[]> {
    return this.request<Tag[]>(`/api/tags/search?q=${encodeURIComponent(query)}`);
  }

  async findDocuments(tags: string[]): Promise<DocumentSummary[]> {
    return this.jsonPost<DocumentSummary[]>("/api/documents/find", { tags });
  }

  async uploadFile(file: File, tags?: string[]): Promise<IngestResult> {
    const form = new FormData();
    form.append("file", file);
    if (tags && tags.length > 0) {
      form.append("tags", JSON.stringify(tags));
    }
    return this.request<IngestResult>("/api/ingest/upload", {
      method: "POST",
      body: form,
    });
  }

  async query(query: string, topK?: number, tags?: string[]): Promise<ChunkResult[]> {
    return this.jsonPost<ChunkResult[]>("/api/query", { query, top_k: topK, tags });
  }

  async ask(query: string, topK?: number, tags?: string[]): Promise<AskResponse> {
    return this.jsonPost<AskResponse>("/api/ask", { query, top_k: topK, tags });
  }

  async agent(message: string, systemPrompt?: string): Promise<{ answer: string }> {
    return this.jsonPost<{ answer: string }>("/api/agent", { message, system_prompt: systemPrompt });
  }
}
