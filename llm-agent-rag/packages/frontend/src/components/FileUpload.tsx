import { useState, useRef } from "react";
import { api } from "../api";
import { TagInput } from "./TagInput";
import { TagPill } from "./TagPill";
import { Spinner } from "./Spinner";
import styles from "./FileUpload.module.css";

interface FileUploadProps {
  onSuccess: () => void;
  onCancel: () => void;
}

export function FileUpload({ onSuccess, onCancel }: FileUploadProps) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [tagRows, setTagRows] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  function addTag(tag: string) {
    const t = tag.trim();
    if (!t || tagRows.includes(t)) return;
    setTagRows([...tagRows, t]);
  }

  function removeTag(tag: string) {
    setTagRows(tagRows.filter((t) => t !== tag));
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const file = fileRef.current?.files?.[0];
    if (!file) {
      setError("Please select a file");
      return;
    }

    setLoading(true);
    setError("");
    try {
      await api.uploadFile(file, tagRows);
      onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  const excludeSet = new Set(tagRows);

  return (
    <div className={styles.overlay}>
      <form className={styles.form} onSubmit={handleSubmit}>
        {loading && <Spinner overlay />}
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
          <span className={styles.label}>Tags</span>
          {tagRows.length > 0 && (
            <div className={styles.tagPills}>
              {tagRows.map((t) => (
                <TagPill key={t} tag={t} onRemove={() => removeTag(t)} />
              ))}
            </div>
          )}
          <TagInput onSelect={addTag} exclude={excludeSet} placeholder="Search or add tag..." />
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
