import type { Tag } from "@rag/shared";
import styles from "./TagFilter.module.css";

interface TagFilterProps {
  tags: Tag[];
  selected: Set<string>;
  onChange: (selected: Set<string>) => void;
}

export function TagFilter({ tags, selected, onChange }: TagFilterProps) {
  if (tags.length === 0) return null;

  function toggle(tag: string) {
    const next = new Set(selected);
    if (next.has(tag)) {
      next.delete(tag);
    } else {
      next.add(tag);
    }
    onChange(next);
  }

  return (
    <div className={styles.container}>
      <span className={styles.label}>Filter by tag:</span>
      <div className={styles.chips}>
        {[...tags].sort((a, b) => a.tag.localeCompare(b.tag)).map((t) => {
          const isActive = selected.has(t.tag);
          return (
            <button
              key={t.tag}
              className={`${styles.chip} ${isActive ? styles.active : ""}`}
              onClick={() => toggle(t.tag)}
            >
              {t.tag}
              <span className={styles.count}>{t.doc_count}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
