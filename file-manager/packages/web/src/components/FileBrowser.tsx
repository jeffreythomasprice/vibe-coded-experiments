import { useId, useRef, useState, useCallback } from "react";
import type { MountInfo } from "@file-manager/schemas";
import { useFiles } from "../hooks/useFiles.js";
import { FileList } from "./FileList.js";
import styles from "./FileBrowser.module.css";

export interface DraggedFile {
    mountId: string;
    path: string;
    name: string;
}

interface Props {
    mountId: string | null;
    path: string;
    mounts: MountInfo[];
    onMountChange: (mountId: string) => void;
    onNavigate: (path: string) => void;
    refreshKey?: number;
    onFileDrop?: (data: DraggedFile) => void;
}

function buildBreadcrumbs(mountId: string, path: string): { label: string; path: string }[] {
    const parts = path.split("/").filter(Boolean);
    const crumbs: { label: string; path: string }[] = [{ label: mountId, path: "/" }];
    let accumulated = "";
    for (const part of parts) {
        accumulated += `/${part}`;
        crumbs.push({ label: part, path: accumulated });
    }
    return crumbs;
}

export function FileBrowser({ mountId, path, mounts, onMountChange, onNavigate, refreshKey, onFileDrop }: Props) {
    const { files, loading, error, upload, remove, mkdir } = useFiles(mountId, path, refreshKey);
    const inputRef = useRef<HTMLInputElement>(null);
    const uploadInputId = useId();
    const [showMkdir, setShowMkdir] = useState(false);
    const [folderName, setFolderName] = useState("");
    const [mkdirError, setMkdirError] = useState<string | null>(null);
    const [isDragOver, setIsDragOver] = useState(false);

    const handleDragOver = useCallback((e: React.DragEvent) => {
        e.preventDefault();
        setIsDragOver(true);
    }, []);

    const handleDragLeave = useCallback(() => {
        setIsDragOver(false);
    }, []);

    const handleDrop = useCallback((e: React.DragEvent) => {
        e.preventDefault();
        setIsDragOver(false);
        const raw = e.dataTransfer.getData("application/json");
        if (!raw || !onFileDrop) return;
        try {
            const data = JSON.parse(raw) as DraggedFile;
            onFileDrop(data);
        } catch {
            // ignore malformed drag data
        }
    }, [onFileDrop]);

    const crumbs = mountId ? buildBreadcrumbs(mountId, path) : [];

    function handleFileChange(e: React.ChangeEvent<HTMLInputElement>) {
        const file = e.target.files?.[0];
        if (!file) return;
        void upload(file).finally(() => {
            if (inputRef.current) inputRef.current.value = "";
        });
    }

    async function handleMkdirSubmit(e: React.FormEvent) {
        e.preventDefault();
        const name = folderName.trim();
        if (!name) return;
        const dirPath = path === "/" ? `/${name}` : `${path}/${name}`;
        try {
            await mkdir(dirPath);
            setShowMkdir(false);
            setFolderName("");
            setMkdirError(null);
        } catch (err) {
            setMkdirError(err instanceof Error ? err.message : String(err));
        }
    }

    function handleMkdirCancel() {
        setShowMkdir(false);
        setFolderName("");
        setMkdirError(null);
    }

    return (
        <div className={styles.browser}>
            <div className={styles.header}>
                <select
                    className={styles.mountSelect}
                    value={mountId ?? ""}
                    onChange={(e) => {
                        if (e.target.value) onMountChange(e.target.value);
                    }}
                >
                    <option value="">— select mount —</option>
                    {mounts.map((m) => (
                        <option key={m.mountId} value={m.mountId}>
                            {m.mountId}
                        </option>
                    ))}
                </select>
                {mountId && (
                    <div className={styles.breadcrumb}>
                        {crumbs.map((crumb, i) => (
                            <span key={crumb.path + String(i)} className={styles.breadcrumbItem}>
                                {i > 0 && <span className={styles.sep}>/</span>}
                                {i < crumbs.length - 1 ? (
                                    <button
                                        className={styles.breadcrumbBtn}
                                        onClick={() => onNavigate(crumb.path)}
                                    >
                                        {crumb.label}
                                    </button>
                                ) : (
                                    <span className={styles.breadcrumbCurrent}>{crumb.label}</span>
                                )}
                            </span>
                        ))}
                    </div>
                )}
            </div>

            {!mountId ? (
                <div className={styles.emptyPane}>
                    <p>Select a mount to browse files.</p>
                </div>
            ) : (
                <>
                    <div
                        className={`${styles.content}${isDragOver ? ` ${styles.dropTarget}` : ""}`}
                        onDragOver={handleDragOver}
                        onDragLeave={handleDragLeave}
                        onDrop={handleDrop}
                    >
                        {loading && <p className={styles.status}>Loading…</p>}
                        {error && <p className={styles.error}>{error}</p>}
                        {!loading && !error && (
                            <FileList
                                mountId={mountId}
                                entries={files}
                                onNavigate={onNavigate}
                                onDelete={(filePath) => {
                                    void remove(filePath);
                                }}
                            />
                        )}
                    </div>

                    <div className={styles.toolbar}>
                        <input
                            ref={inputRef}
                            type="file"
                            id={uploadInputId}
                            className={styles.fileInput}
                            onChange={handleFileChange}
                        />
                        <label htmlFor={uploadInputId} className={styles.uploadLabel}>
                            Upload file
                        </label>
                        <button className={styles.mkdirBtn} onClick={() => setShowMkdir(true)}>
                            New Folder
                        </button>
                        {showMkdir && (
                            <form className={styles.mkdirForm} onSubmit={(e) => { void handleMkdirSubmit(e); }}>
                                <input
                                    className={styles.mkdirInput}
                                    type="text"
                                    placeholder="Folder name"
                                    value={folderName}
                                    onChange={(e) => setFolderName(e.target.value)}
                                    autoFocus
                                />
                                <button type="submit" className={styles.mkdirSubmit}>Create</button>
                                <button type="button" className={styles.mkdirCancel} onClick={handleMkdirCancel}>Cancel</button>
                                {mkdirError && <span className={styles.mkdirError}>{mkdirError}</span>}
                            </form>
                        )}
                    </div>
                </>
            )}
        </div>
    );
}
