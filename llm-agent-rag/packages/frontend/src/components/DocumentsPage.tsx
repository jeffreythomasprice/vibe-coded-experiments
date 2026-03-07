import { useState, useEffect, useCallback } from "react";
import type { Document, Tag } from "@rag/shared";
import { api } from "../api";
import { TagFilter } from "./TagFilter";
import { FileUpload } from "./FileUpload";
import styles from "./DocumentsPage.module.css";

export function DocumentsPage() {
  const [docs, setDocs] = useState<Document[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [selectedTags, setSelectedTags] = useState<Record<string, string>>({});
  const [showUpload, setShowUpload] = useState(false);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [d, t] = await Promise.all([api.listDocuments(), api.listTags()]);
      setDocs(d);
      setTags(t);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function handleDelete(id: number, name: string) {
    if (!confirm(`Delete "${name}" and all its chunks?`)) return;
    await api.deleteDocument(id);
    setDocs((prev) => prev.filter((d) => d.id !== id));
    // Refresh tags since counts may have changed
    api.listTags().then(setTags);
  }

  const filtered = docs.filter((doc) =>
    Object.entries(selectedTags).every(
      ([k, v]) => doc.tags[k] === v,
    ),
  );

  return (
    <div>
      <div className={styles.header}>
        <h2 className={styles.heading}>Documents</h2>
        <button className={styles.ingestBtn} onClick={() => setShowUpload(true)}>
          Ingest File
        </button>
      </div>

      <TagFilter tags={tags} selected={selectedTags} onChange={setSelectedTags} />

      {loading ? (
        <p className={styles.info}>Loading...</p>
      ) : filtered.length === 0 ? (
        <p className={styles.info}>No documents found.</p>
      ) : (
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Name</th>
              <th>Ingested At</th>
              <th>Tags</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((doc) => (
              <tr key={doc.id}>
                <td>{doc.name}</td>
                <td className={styles.date}>
                  {new Date(doc.ingested_at).toLocaleString()}
                </td>
                <td>
                  <div className={styles.tagList}>
                    {Object.entries(doc.tags).map(([k, v]) => (
                      <span key={k} className={styles.badge}>
                        {k}={v}
                      </span>
                    ))}
                  </div>
                </td>
                <td>
                  <button
                    className={styles.deleteBtn}
                    onClick={() => handleDelete(doc.id, doc.name)}
                  >
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {showUpload && (
        <FileUpload
          onSuccess={() => {
            setShowUpload(false);
            refresh();
          }}
          onCancel={() => setShowUpload(false)}
        />
      )}
    </div>
  );
}
