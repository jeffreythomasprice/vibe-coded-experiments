import { useOperations } from "../hooks/useOperations.js";
import { ProgressBar } from "./ProgressBar.js";
import styles from "./QueueTab.module.css";

function statusOrder(status: string): number {
    switch (status) {
        case "in_progress": return 0;
        case "pending": return 1;
        case "completed": return 2;
        case "failed": return 3;
        default: return 4;
    }
}

export function QueueTab() {
    const { operations } = useOperations();

    const sorted = Array.from(operations.values()).sort((a, b) => {
        const so = statusOrder(a.status) - statusOrder(b.status);
        if (so !== 0) return so;
        return new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime();
    });

    if (sorted.length === 0) {
        return (
            <div className={styles.empty}>
                No operations yet. Upload, copy, move, or delete files to see progress here.
            </div>
        );
    }

    return (
        <div className={styles.container}>
            {sorted.map((op) => (
                <div key={op.id} className={styles.row}>
                    <div className={styles.header}>
                        <span className={styles.typeLabel}>{op.type.toUpperCase()}</span>
                        <span className={`${styles.badge} ${styles[op.status] ?? ""}`}>
                            {op.status.replace("_", " ")}
                        </span>
                    </div>
                    <div className={styles.paths}>
                        <span className={styles.pathValue} title={op.src}>{op.src}</span>
                        {op.dest != null && (
                            <>
                                <span className={styles.arrow}>&rarr;</span>
                                <span className={styles.pathValue} title={op.dest}>{op.dest}</span>
                            </>
                        )}
                    </div>
                    {(op.status === "pending" || op.status === "in_progress") && (
                        <ProgressBar
                            processedBytes={op.processedBytes}
                            totalBytes={op.totalBytes}
                            processedFiles={op.processedFiles}
                            totalFiles={op.totalFiles}
                            currentFile={op.currentFile}
                        />
                    )}
                    {op.status === "failed" && op.error && (
                        <div className={styles.error}>{op.error}</div>
                    )}
                </div>
            ))}
        </div>
    );
}
