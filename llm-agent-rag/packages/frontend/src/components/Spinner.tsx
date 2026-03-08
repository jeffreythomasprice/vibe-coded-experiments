import styles from "./Spinner.module.css";

interface SpinnerProps {
  overlay?: boolean;
}

export function Spinner({ overlay }: SpinnerProps) {
  if (overlay) {
    return (
      <div className={styles.overlay}>
        <div className={styles.spinner} />
      </div>
    );
  }
  return <div className={styles.spinner} />;
}
