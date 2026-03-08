import { useState, useEffect } from "react";
import type { ChunkResult, Tag, DocumentSummary } from "@rag/shared";
import { api } from "../api";
import { TagFilter } from "./TagFilter";
import { Spinner } from "./Spinner";
import styles from "./QueryPage.module.css";

export function QueryPage() {
  const [query, setQuery] = useState("");
  const [topK, setTopK] = useState(5);
  const [tags, setTags] = useState<Tag[]>([]);
  const [selectedTags, setSelectedTags] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [matchingDocs, setMatchingDocs] = useState<DocumentSummary[]>([]);
  const [chunks, setChunks] = useState<ChunkResult[] | null>(null);

  useEffect(() => {
    api.listTags().then(setTags);
  }, []);

  useEffect(() => {
    if (selectedTags.size === 0) {
      setMatchingDocs([]);
      return;
    }
    api.findDocuments([...selectedTags]).then(setMatchingDocs);
  }, [selectedTags]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!query.trim()) return;

    setLoading(true);
    setChunks(null);
    setError("");

    try {
      const tagFilter = selectedTags.size > 0 ? [...selectedTags] : undefined;
      const results = await api.query(query, topK, tagFilter);
      setChunks(results);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className={styles.container}>
      {loading && <Spinner overlay />}
      <h2 className={styles.heading}>Query</h2>

      <form onSubmit={handleSubmit}>
        <textarea
          className={styles.textarea}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Enter your query..."
          rows={3}
          disabled={loading}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              handleSubmit(e);
            }
          }}
        />

        <div className={styles.options}>
          <label className={styles.topKLabel}>
            Top K:
            <input
              type="number"
              min={1}
              max={50}
              value={topK}
              onChange={(e) => setTopK(parseInt(e.target.value) || 5)}
              className={styles.topKInput}
              disabled={loading}
            />
          </label>
          <TagFilter tags={tags} selected={selectedTags} onChange={setSelectedTags} />
        </div>

        {matchingDocs.length > 0 && (
          <div className={styles.matchingDocs}>
            <div className={styles.matchingDocsLabel}>
              Searching {matchingDocs.length} document{matchingDocs.length !== 1 ? "s" : ""}:
            </div>
            <ul className={styles.matchingDocsList}>
              {matchingDocs.map((d) => (
                <li key={d.id}>{d.name}</li>
              ))}
            </ul>
          </div>
        )}

        <button type="submit" className={styles.submitBtn} disabled={loading || !query.trim()}>
          {loading ? "Processing..." : "Submit"}
        </button>
      </form>

      {error && <p className={styles.error}>{error}</p>}

      {chunks && (
        <div className={styles.results}>
          <h3>Results ({chunks.length} chunks)</h3>
          <table className={styles.table}>
            <thead>
              <tr>
                <th>Document</th>
                <th>Chunk</th>
                <th>Pages</th>
                <th>Similarity</th>
                <th>Content</th>
              </tr>
            </thead>
            <tbody>
              {chunks.map((c) => (
                <tr key={c.chunk_id}>
                  <td>{c.document_name}</td>
                  <td className={styles.center}>{c.chunk_index}</td>
                  <td className={styles.center}>
                    {c.page_start
                      ? c.page_start === c.page_end
                        ? `Page ${c.page_start}`
                        : `Pages ${c.page_start}-${c.page_end}`
                      : "\u2014"}
                  </td>
                  <td className={styles.center}>{(c.similarity * 100).toFixed(1)}%</td>
                  <td className={styles.content}>
                    {c.content.length > 200 ? c.content.slice(0, 200) + "..." : c.content}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
