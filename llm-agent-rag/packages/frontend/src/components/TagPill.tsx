import styles from "./TagPill.module.css";

interface TagPillProps {
  tag: string;
  onRemove?: () => void;
  onClick?: () => void;
  active?: boolean;
}

export function TagPill({ tag, onRemove, onClick, active }: TagPillProps) {
  return (
    <span
      className={`${styles.pill} ${active ? styles.active : ""} ${onClick ? styles.clickable : ""}`}
      onClick={onClick}
    >
      {tag}
      {onRemove && (
        <button
          className={styles.remove}
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
        >
          ×
        </button>
      )}
    </span>
  );
}
