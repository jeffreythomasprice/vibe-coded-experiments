import type { Tag } from "@rag/shared";
import styles from "./TagFilter.module.css";

interface TagFilterProps {
  tags: Tag[];
  selected: Record<string, string>;
  onChange: (selected: Record<string, string>) => void;
}

export function TagFilter({ tags, selected, onChange }: TagFilterProps) {
  if (tags.length === 0) return null;

  function toggle(key: string, value: string) {
    const next = { ...selected };
    if (next[key] === value) {
      delete next[key];
    } else {
      next[key] = value;
    }
    onChange(next);
  }

  return (
    <div className={styles.container}>
      <span className={styles.label}>Filter by tag:</span>
      <div className={styles.chips}>
        {tags.map((t) => {
          const isActive = selected[t.key] === t.value;
          return (
            <button
              key={`${t.key}=${t.value}`}
              className={`${styles.chip} ${isActive ? styles.active : ""}`}
              onClick={() => toggle(t.key, t.value)}
            >
              {t.key}={t.value}
              <span className={styles.count}>{t.doc_count}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
