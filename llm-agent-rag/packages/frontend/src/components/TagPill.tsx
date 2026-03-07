import styles from "./TagPill.module.css";

interface TagPillProps {
  tagKey: string;
  tagValue: string;
  onRemove?: () => void;
  onClick?: () => void;
  active?: boolean;
}

export function TagPill({ tagKey, tagValue, onRemove, onClick, active }: TagPillProps) {
  return (
    <span
      className={`${styles.pill} ${active ? styles.active : ""} ${onClick ? styles.clickable : ""}`}
      onClick={onClick}
    >
      {tagKey}={tagValue}
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
