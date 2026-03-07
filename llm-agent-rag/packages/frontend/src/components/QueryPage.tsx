import { useState, useEffect } from "react";
import type { ChunkResult, AskResponse, Tag } from "@rag/shared";
import { api } from "../api";
import { TagFilter } from "./TagFilter";
import styles from "./QueryPage.module.css";

type Mode = "search" | "ask" | "agent";

export function QueryPage() {
  const [mode, setMode] = useState<Mode>("search");
  const [query, setQuery] = useState("");
  const [topK, setTopK] = useState(5);
  const [tags, setTags] = useState<Tag[]>([]);
  const [selectedTags, setSelectedTags] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  // Results
  const [chunks, setChunks] = useState<ChunkResult[] | null>(null);
  const [askResult, setAskResult] = useState<AskResponse | null>(null);
  const [agentAnswer, setAgentAnswer] = useState<string | null>(null);

  useEffect(() => {
    api.listTags().then(setTags);
  }, []);

  function clearResults() {
    setChunks(null);
    setAskResult(null);
    setAgentAnswer(null);
    setError("");
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!query.trim()) return;

    setLoading(true);
    clearResults();

    try {
      const tagFilter = Object.keys(selectedTags).length > 0 ? selectedTags : undefined;

      if (mode === "search") {
        const results = await api.query(query, topK, tagFilter);
        setChunks(results);
      } else if (mode === "ask") {
        const result = await api.ask(query, topK, tagFilter);
        setAskResult(result);
      } else {
        const result = await api.agent(query);
        setAgentAnswer(result.answer);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div>
      <h2 className={styles.heading}>Query</h2>

      <div className={styles.modes}>
        {(["search", "ask", "agent"] as Mode[]).map((m) => (
          <label key={m} className={styles.modeLabel}>
            <input
              type="radio"
              name="mode"
              value={m}
              checked={mode === m}
              onChange={() => { setMode(m); clearResults(); }}
            />
            {m.charAt(0).toUpperCase() + m.slice(1)}
          </label>
        ))}
      </div>

      <form onSubmit={handleSubmit}>
        <textarea
          className={styles.textarea}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={mode === "agent" ? "Enter a message..." : "Enter your query..."}
          rows={3}
          disabled={loading}
        />

        {mode !== "agent" && (
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
        )}

        <button type="submit" className={styles.submitBtn} disabled={loading || !query.trim()}>
          {loading ? "Processing..." : "Submit"}
        </button>
      </form>

      {error && <p className={styles.error}>{error}</p>}

      {/* Search results */}
      {chunks && (
        <div className={styles.results}>
          <h3>Results ({chunks.length} chunks)</h3>
          <table className={styles.table}>
            <thead>
              <tr>
                <th>Document</th>
                <th>Chunk</th>
                <th>Similarity</th>
                <th>Content</th>
              </tr>
            </thead>
            <tbody>
              {chunks.map((c) => (
                <tr key={c.chunk_id}>
                  <td>{c.document_name}</td>
                  <td className={styles.center}>{c.chunk_index}</td>
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

      {/* Ask results */}
      {askResult && (
        <div className={styles.results}>
          <h3>Answer</h3>
          <div className={styles.answer}>{askResult.answer}</div>
          {askResult.sources.length > 0 && (
            <>
              <h4>Sources</h4>
              <ul className={styles.sources}>
                {askResult.sources.map((s, i) => (
                  <li key={i}>
                    <strong>{s.document}</strong> (chunk {s.chunk_index}, {(s.similarity * 100).toFixed(1)}%)
                    <p className={styles.excerpt}>{s.excerpt}</p>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}

      {/* Agent results */}
      {agentAnswer !== null && (
        <div className={styles.results}>
          <h3>Answer</h3>
          <div className={styles.answer}>{agentAnswer}</div>
        </div>
      )}
    </div>
  );
}
