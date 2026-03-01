import { useOperations } from "../hooks/useOperations.js";
import styles from "./StatusIndicator.module.css";

interface Props {
    onClick: () => void;
}

export function StatusIndicator({ onClick }: Props) {
    const { activeCount } = useOperations();

    if (activeCount === 0) return null;

    return (
        <button className={styles.indicator} onClick={onClick}>
            {activeCount} operation{activeCount !== 1 ? "s" : ""} in progress
        </button>
    );
}
