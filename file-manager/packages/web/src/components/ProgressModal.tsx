import { useEffect } from "react";
import { useOperations } from "../hooks/useOperations.js";
import { ProgressBar } from "./ProgressBar.js";
import styles from "./ProgressModal.module.css";

interface Props {
    operationId: string;
    onDismiss: () => void;
}

const typeLabels: Record<string, string> = {
    copy: "Copying",
    move: "Moving",
    upload: "Uploading",
    delete: "Deleting",
};

export function ProgressModal({ operationId, onDismiss }: Props) {
    const { operations } = useOperations();
    const op = operations.get(operationId);

    useEffect(() => {
        if (op?.status !== "completed") return;
        const timer = setTimeout(onDismiss, 800);
        return () => clearTimeout(timer);
    }, [op?.status, onDismiss]);

    if (!op) return null;

    const typeLabel = typeLabels[op.type] ?? op.type;
    const isDone = op.status === "completed" || op.status === "failed";

    function handleOverlayClick(e: React.MouseEvent<HTMLDivElement>) {
        if (e.target === e.currentTarget) {
            onDismiss();
        }
    }

    return (
        <div className={styles.overlay} onClick={handleOverlayClick}>
            <div className={styles.window}>
                <div className={styles.titleBar}>
                    {isDone ? (op.status === "completed" ? "Complete" : "Failed") : typeLabel}
                </div>
                <div className={styles.body}>
                    <div className={styles.paths}>
                        <div className={styles.pathRow}>
                            <span className={styles.pathLabel}>From:</span>
                            <span className={styles.pathValue} title={op.src}>{op.src}</span>
                        </div>
                        {op.dest && (
                            <div className={styles.pathRow}>
                                <span className={styles.pathLabel}>To:</span>
                                <span className={styles.pathValue} title={op.dest}>{op.dest}</span>
                            </div>
                        )}
                    </div>
                    <ProgressBar
                        processedBytes={op.processedBytes}
                        totalBytes={op.totalBytes}
                        processedFiles={op.processedFiles}
                        totalFiles={op.totalFiles}
                        currentFile={op.currentFile}
                    />
                    {op.status === "failed" && op.error && (
                        <div className={styles.error}>{op.error}</div>
                    )}
                </div>
                <div className={styles.footer}>
                    <button className={styles.dismissBtn} onClick={onDismiss}>
                        {isDone ? "Close" : "Continue Working"}
                    </button>
                </div>
            </div>
        </div>
    );
}
