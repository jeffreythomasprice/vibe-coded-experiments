import type { FileEntry } from "@file-manager/schemas";
import { downloadFile } from "../api/client.js";
import styles from "./FileList.module.css";

interface Props {
    mountId: string;
    entries: FileEntry[];
    onNavigate: (path: string) => void;
    onDelete: (path: string) => void;
}

function formatSize(bytes: number): string {
    if (bytes === 0) return "—";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function FileList({ mountId, entries, onNavigate, onDelete }: Props) {
    if (entries.length === 0) {
        return <p className={styles.empty}>No files here.</p>;
    }

    return (
        <table className={styles.table}>
            <thead>
                <tr>
                    <th className={styles.colName}>Name</th>
                    <th className={styles.colSize}>Size</th>
                    <th className={styles.colActions}>Actions</th>
                </tr>
            </thead>
            <tbody>
                {entries.map((entry) => (
                    <tr key={entry.path} className={styles.row}>
                        <td className={styles.cellName}>
                            {entry.type === "directory" ? (
                                <button
                                    className={styles.dirBtn}
                                    onClick={() => onNavigate(entry.path)}
                                >
                                    <span className={styles.fileIcon}>&#128193;</span>
                                    {entry.name}
                                </button>
                            ) : (
                                <span className={styles.fileName}>
                                    <span className={styles.fileIcon}>&#128196;</span>
                                    {entry.name}
                                </span>
                            )}
                        </td>
                        <td className={styles.cellSize}>
                            {entry.type === "directory" ? "—" : formatSize(entry.size)}
                        </td>
                        <td className={styles.cellActions}>
                            {entry.type === "file" && (
                                <button
                                    className={styles.actionBtn}
                                    title="Download"
                                    onClick={() => downloadFile(mountId, entry.path, entry.name)}
                                >
                                    &#8595;
                                </button>
                            )}
                            <button
                                className={`${styles.actionBtn} ${styles.deleteBtn}`}
                                title="Delete"
                                onClick={() => onDelete(entry.path)}
                            >
                                &times;
                            </button>
                        </td>
                    </tr>
                ))}
            </tbody>
        </table>
    );
}
