import { useState, useRef, useEffect } from "react";
import type { Tag } from "@rag/shared";
import { api } from "../api";
import styles from "./TagInput.module.css";

interface TagInputProps {
  onSelect: (tag: string) => void;
  placeholder?: string;
  exclude?: Set<string>;
}

export function TagInput({ onSelect, placeholder = "Search or add tag...", exclude }: TagInputProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Tag[]>([]);
  const [open, setOpen] = useState(false);
  const [highlightIdx, setHighlightIdx] = useState(-1);
  const containerRef = useRef<HTMLDivElement>(null);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  function handleChange(value: string) {
    setQuery(value);
    if (timerRef.current !== null) clearTimeout(timerRef.current);
    if (!value.trim()) {
      setResults([]);
      setOpen(false);
      return;
    }
    timerRef.current = window.setTimeout(async () => {
      let tags = await api.searchTags(value.trim());
      if (exclude) tags = tags.filter((t) => !exclude.has(t.tag));
      setResults(tags);
      setHighlightIdx(-1);
      setOpen(true);
    }, 300);
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "ArrowDown" && open && results.length > 0) {
      e.preventDefault();
      setHighlightIdx((i) => (i + 1) % results.length);
      return;
    }
    if (e.key === "ArrowUp" && open && results.length > 0) {
      e.preventDefault();
      setHighlightIdx((i) => (i <= 0 ? results.length - 1 : i - 1));
      return;
    }
    if (e.key === "Escape") {
      setOpen(false);
      setHighlightIdx(-1);
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (open && highlightIdx >= 0 && highlightIdx < results.length) {
        handleSelect(results[highlightIdx]);
      } else if (query.trim()) {
        onSelect(query.trim());
        setQuery("");
        setOpen(false);
      }
      setHighlightIdx(-1);
    }
  }

  function handleSelect(tag: Tag) {
    onSelect(tag.tag);
    setQuery("");
    setOpen(false);
  }

  return (
    <div className={styles.container} ref={containerRef}>
      <input
        className={styles.input}
        type="text"
        value={query}
        onChange={(e) => handleChange(e.target.value)}
        onKeyDown={handleKeyDown}
        onFocus={() => { if (results.length > 0) setOpen(true); }}
        placeholder={placeholder}
      />
      {open && results.length > 0 && (
        <ul className={styles.dropdown}>
          {results.map((t, i) => (
            <li
              key={t.tag}
              className={`${styles.item}${i === highlightIdx ? ` ${styles.highlighted}` : ""}`}
              onClick={() => handleSelect(t)}
            >
              <span>{t.tag}</span>
              <span className={styles.count}>{t.doc_count}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
