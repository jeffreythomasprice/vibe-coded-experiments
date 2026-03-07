import { useState, useRef } from "react";
import { api } from "../api";
import styles from "./FileUpload.module.css";

interface FileUploadProps {
  onSuccess: () => void;
  onCancel: () => void;
}

interface TagRow {
  key: string;
  value: string;
}

export function FileUpload({ onSuccess, onCancel }: FileUploadProps) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [tagRows, setTagRows] = useState<TagRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  function addTag() {
    setTagRows([...tagRows, { key: "", value: "" }]);
  }

  function removeTag(i: number) {
    setTagRows(tagRows.filter((_, idx) => idx !== i));
  }

  function updateTag(i: number, field: "key" | "value", val: string) {
    const next = [...tagRows];
    next[i] = { ...next[i], [field]: val };
    setTagRows(next);
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const file = fileRef.current?.files?.[0];
    if (!file) {
      setError("Please select a file");
      return;
    }

    const tags: Record<string, string> = {};
    for (const row of tagRows) {
      if (row.key.trim()) {
        tags[row.key.trim()] = row.value.trim();
      }
    }

    setLoading(true);
    setError("");
    try {
      await api.uploadFile(file, tags);
      onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className={styles.overlay}>
      <form className={styles.form} onSubmit={handleSubmit}>
        <h3 className={styles.title}>Ingest File</h3>

        <label className={styles.label}>
          File
          <input
            ref={fileRef}
            type="file"
            accept=".pdf,.txt"
            className={styles.fileInput}
            disabled={loading}
          />
        </label>

        <div className={styles.tagsSection}>
          <div className={styles.tagsHeader}>
            <span className={styles.label}>Tags</span>
            <button type="button" className={styles.addBtn} onClick={addTag} disabled={loading}>
              + Add Tag
            </button>
          </div>
          {tagRows.map((row, i) => (
            <div key={i} className={styles.tagRow}>
              <input
                placeholder="key"
                value={row.key}
                onChange={(e) => updateTag(i, "key", e.target.value)}
                className={styles.tagInput}
                disabled={loading}
              />
              <span>=</span>
              <input
                placeholder="value"
                value={row.value}
                onChange={(e) => updateTag(i, "value", e.target.value)}
                className={styles.tagInput}
                disabled={loading}
              />
              <button type="button" className={styles.removeBtn} onClick={() => removeTag(i)} disabled={loading}>
                x
              </button>
            </div>
          ))}
        </div>

        {error && <p className={styles.error}>{error}</p>}

        <div className={styles.actions}>
          <button type="button" className={styles.cancelBtn} onClick={onCancel} disabled={loading}>
            Cancel
          </button>
          <button type="submit" className={styles.submitBtn} disabled={loading}>
            {loading ? "Ingesting..." : "Upload & Ingest"}
          </button>
        </div>
      </form>
    </div>
  );
}
