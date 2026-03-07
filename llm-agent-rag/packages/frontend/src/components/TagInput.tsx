import { useState, useRef, useEffect } from "react";
import type { Tag } from "@rag/shared";
import { api } from "../api";
import styles from "./TagInput.module.css";

interface TagInputProps {
  onSelect: (key: string, value: string) => void;
  placeholder?: string;
}

export function TagInput({ onSelect, placeholder = "Search or add tag..." }: TagInputProps) {
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
      const tags = await api.searchTags(value.trim());
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
      } else if (query.includes("=")) {
        const idx = query.indexOf("=");
        const key = query.slice(0, idx).trim();
        const value = query.slice(idx + 1).trim();
        if (key && value) {
          onSelect(key, value);
          setQuery("");
          setOpen(false);
        }
      } else if (open && results.length === 1) {
        handleSelect(results[0]);
      }
      setHighlightIdx(-1);
    }
  }

  function handleSelect(tag: Tag) {
    onSelect(tag.key, tag.value);
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
              key={`${t.key}=${t.value}`}
              className={`${styles.item}${i === highlightIdx ? ` ${styles.highlighted}` : ""}`}
              onClick={() => handleSelect(t)}
            >
              <span>{t.key}={t.value}</span>
              <span className={styles.count}>{t.doc_count}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
