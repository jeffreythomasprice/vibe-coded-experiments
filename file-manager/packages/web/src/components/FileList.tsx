import type { FileEntry } from "@file-manager/schemas";
import { downloadFile } from "../api/client.js";
import styles from "./FileList.module.css";

interface Props {
    mountId: string;
    entries: FileEntry[];
    parentPath: string | null;
    onNavigate: (path: string) => void;
    onDelete: (path: string) => void;
    onRename?: (entry: FileEntry) => void;
    onContextMenu?: (e: React.MouseEvent, entry: FileEntry) => void;
    cutPath?: string | null;
}

function formatSize(bytes: number): string {
    if (bytes === 0) return "0 B";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function FileList({ mountId, entries, parentPath, onNavigate, onDelete, onRename, onContextMenu, cutPath }: Props) {
    if (entries.length === 0 && !parentPath) {
        return <p className={styles.empty}>No files here.</p>;
    }

    return (
        <table className={styles.table}>
            <thead>
                <tr>
                    <th className={styles.colName}>Name</th>
                    <th className={styles.colSize}>Size</th>
                    <th className={styles.colActions}></th>
                </tr>
            </thead>
            <tbody>
                {parentPath !== null && (
                    <tr className={`${styles.row} ${styles.upRow}`}>
                        <td className={styles.cellName} colSpan={2}>
                            <button className={styles.dirBtn} onClick={() => onNavigate(parentPath)}>
                                ..
                            </button>
                        </td>
                        <td className={styles.cellActions} />
                    </tr>
                )}
                {entries.map((entry) => (
                    <tr
                        key={entry.path}
                        className={`${styles.row}${cutPath === entry.path ? ` ${styles.cut}` : ""}`}
                        draggable
                        onContextMenu={(e) => { e.stopPropagation(); e.preventDefault(); onContextMenu?.(e, entry); }}
                        onDragStart={(e) => {
                            e.dataTransfer.setData(
                                "application/json",
                                JSON.stringify({ mountId, path: entry.path, name: entry.name }),
                            );
                            e.dataTransfer.effectAllowed = "move";
                        }}
                    >
                        <td className={styles.cellName}>
                            {entry.type === "directory" ? (
                                <button className={styles.dirBtn} onClick={() => onNavigate(entry.path)}>
                                    {entry.name}
                                </button>
                            ) : (
                                <span className={styles.fileName}>{entry.name}</span>
                            )}
                        </td>
                        <td className={styles.cellSize}>
                            {entry.type === "directory"
                                ? <span className={styles.dirTag}>&lt;DIR&gt;</span>
                                : formatSize(entry.size)}
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
                                className={styles.actionBtn}
                                title="Rename"
                                onClick={() => onRename?.(entry)}
                            >
                                &#9998;
                            </button>
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
