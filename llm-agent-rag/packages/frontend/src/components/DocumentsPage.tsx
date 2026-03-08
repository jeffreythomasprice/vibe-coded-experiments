import { useState, useEffect, useCallback } from "react";
import type { Document, Tag } from "@rag/shared";
import { api } from "../api";
import { TagFilter } from "./TagFilter";
import { TagPill } from "./TagPill";
import { FileUpload } from "./FileUpload";
import { AddTagModal } from "./AddTagModal";
import { ConfirmModal } from "./ConfirmModal";
import { Spinner } from "./Spinner";
import styles from "./DocumentsPage.module.css";

export function DocumentsPage() {
  const [docs, setDocs] = useState<Document[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [selectedTags, setSelectedTags] = useState<Set<string>>(new Set());
  const [showUpload, setShowUpload] = useState(false);
  const [editTagDocId, setEditTagDocId] = useState<number | null>(null);
  const [deleteDoc, setDeleteDoc] = useState<{ id: number; name: string } | null>(null);
  const [loading, setLoading] = useState(true);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [d, t] = await Promise.all([api.listDocuments(), api.listTags()]);
      setDocs(d);
      setTags(t);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load documents");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function handleDelete(id: number) {
    setDeleting(true);
    try {
      await api.deleteDocument(id);
      setDocs((prev) => prev.filter((d) => d.id !== id));
      setDeleteDoc(null);
      api.listTags().then(setTags);
    } finally {
      setDeleting(false);
    }
  }

  const filtered = docs
    .filter((doc) => [...selectedTags].every((t) => doc.tags.includes(t)))
    .sort((a, b) => a.name.localeCompare(b.name) || new Date(a.ingested_at).getTime() - new Date(b.ingested_at).getTime());

  const editTagDoc = editTagDocId !== null ? docs.find((d) => d.id === editTagDocId) : null;

  return (
    <div>
      <div className={styles.header}>
        <h2 className={styles.heading}>Documents</h2>
        <button className={styles.ingestBtn} onClick={() => setShowUpload(true)}>
          Ingest File
        </button>
      </div>

      <TagFilter tags={tags} selected={selectedTags} onChange={setSelectedTags} />

      {error && <p className={styles.error}>{error}</p>}

      {loading ? (
        <div className={styles.spinnerWrap}><Spinner /></div>
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
                    {[...doc.tags].sort((a, b) => a.localeCompare(b)).map((t) => (
                      <TagPill key={t} tag={t} />
                    ))}
                  </div>
                </td>
                <td>
                  <div className={styles.actions}>
                    <button
                      className={styles.tagsBtn}
                      onClick={() => setEditTagDocId(doc.id)}
                    >
                      Tags
                    </button>
                    <button
                      className={styles.deleteBtn}
                      onClick={() => setDeleteDoc({ id: doc.id, name: doc.name })}
                    >
                      Delete
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {editTagDoc && (
        <AddTagModal
          tags={editTagDoc.tags}
          onAddTag={async (tag) => {
            await api.addTag(editTagDoc.id, tag);
            setDocs((prev) =>
              prev.map((d) =>
                d.id === editTagDoc.id
                  ? { ...d, tags: [...d.tags, tag] }
                  : d,
              ),
            );
            api.listTags().then(setTags);
          }}
          onDeleteTag={async (tag) => {
            await api.deleteTag(editTagDoc.id, tag);
            setDocs((prev) =>
              prev.map((d) =>
                d.id === editTagDoc.id
                  ? { ...d, tags: d.tags.filter((t) => t !== tag) }
                  : d,
              ),
            );
            api.listTags().then(setTags);
          }}
          onClose={() => setEditTagDocId(null)}
        />
      )}

      {deleteDoc && (
        <ConfirmModal
          title="Delete Document"
          message={`Delete "${deleteDoc.name}" and all its chunks?`}
          loading={deleting}
          onConfirm={() => handleDelete(deleteDoc.id)}
          onCancel={() => setDeleteDoc(null)}
        />
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
