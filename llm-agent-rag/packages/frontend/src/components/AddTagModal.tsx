import { useState } from "react";
import { TagInput } from "./TagInput";
import styles from "./AddTagModal.module.css";

interface TagEditorModalProps {
  tags: string[];
  onAddTag: (tag: string) => Promise<void>;
  onDeleteTag: (tag: string) => Promise<void>;
  onClose: () => void;
}

export function AddTagModal({ tags, onAddTag, onDeleteTag, onClose }: TagEditorModalProps) {
  const [error, setError] = useState("");
  const [busyTag, setBusyTag] = useState<string | null>(null);

  async function handleAdd(tag: string) {
    if (tags.includes(tag)) return;
    setBusyTag(tag);
    setError("");
    try {
      await onAddTag(tag);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyTag(null);
    }
  }

  async function handleDelete(tag: string) {
    setBusyTag(tag);
    setError("");
    try {
      await onDeleteTag(tag);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyTag(null);
    }
  }

  const excludeSet = new Set(tags);

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h3 className={styles.title}>Edit Tags</h3>

        {tags.length > 0 && (
          <ul className={styles.tagList}>
            {[...tags].sort((a, b) => a.localeCompare(b)).map((t) => (
              <li key={t} className={styles.tagRow}>
                <span className={styles.tagName}>{t}</span>
                <button
                  className={styles.deleteTagBtn}
                  onClick={() => handleDelete(t)}
                  disabled={busyTag !== null}
                >
                  Delete
                </button>
              </li>
            ))}
          </ul>
        )}

        {tags.length === 0 && <p className={styles.emptyMsg}>No tags yet.</p>}

        <div className={styles.addSection}>
          <label className={styles.label}>Add tag</label>
          <TagInput onSelect={handleAdd} exclude={excludeSet} placeholder="Search or create tag..." />
        </div>

        {error && <p className={styles.error}>{error}</p>}

        <div className={styles.actions}>
          <button className={styles.closeBtn} onClick={onClose} disabled={busyTag !== null}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
