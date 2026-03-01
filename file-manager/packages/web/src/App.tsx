import { useState, useCallback, useEffect } from "react";
import { useMounts } from "./hooks/useMounts.js";
import { FileBrowser } from "./components/FileBrowser.js";
import type { DraggedFile } from "./components/FileBrowser.js";
import { MountsTab } from "./components/MountsTab.js";
import { QueueTab } from "./components/QueueTab.js";
import { ContextMenu } from "./components/ContextMenu.js";
import type { MenuItem } from "./components/ContextMenu.js";
import type { FileEntry } from "@file-manager/schemas";
import { moveFile, copyFile, deleteFile, downloadFile, buildFileUri } from "./api/client.js";
import { useToast } from "./hooks/useToast.js";
import { useModal } from "./components/Modal.js";
import { useOperations } from "./hooks/useOperations.js";
import { ProgressModal } from "./components/ProgressModal.js";
import { StatusIndicator } from "./components/StatusIndicator.js";
import { usePersistentState } from "./hooks/usePersistentState.js";
import styles from "./App.module.css";

type Tab = "commander" | "mounts" | "queue";
type Pane = "left" | "right";

interface PaneState {
    mountId: string | null;
    path: string;
}

interface ClipboardEntry {
    operation: "copy" | "cut";
    mountId: string;
    path: string;
    name: string;
}

interface ContextMenuState {
    x: number;
    y: number;
    items: MenuItem[];
}

export function App() {
    const mounts = useMounts();
    const { showError } = useToast();
    const modal = useModal();
    const { trackOperation, onOperationComplete } = useOperations();
    const [activeTab, setActiveTab] = useState<Tab>("commander");
    const [progressOperationId, setProgressOperationId] = useState<string | null>(null);
    const [left, setLeft] = usePersistentState<PaneState>("fm:pane:left", { mountId: null, path: "/" });
    const [right, setRight] = usePersistentState<PaneState>("fm:pane:right", { mountId: null, path: "/" });
    const [leftRefreshKey, setLeftRefreshKey] = useState(0);
    const [rightRefreshKey, setRightRefreshKey] = useState(0);
    const [clipboard, setClipboard] = useState<ClipboardEntry | null>(null);
    const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);

    const closeContextMenu = useCallback(() => setContextMenu(null), []);

    useEffect(() => {
        return onOperationComplete(() => {
            setLeftRefreshKey((k) => k + 1);
            setRightRefreshKey((k) => k + 1);
        });
    }, [onOperationComplete]);

    const startOperation = useCallback((operationId: string) => {
        trackOperation(operationId);
        setProgressOperationId(operationId);
    }, [trackOperation]);

    const refreshPane = useCallback((pane: Pane) => {
        if (pane === "left") setLeftRefreshKey((k) => k + 1);
        else setRightRefreshKey((k) => k + 1);
    }, []);

    const refreshBoth = useCallback(() => {
        setLeftRefreshKey((k) => k + 1);
        setRightRefreshKey((k) => k + 1);
    }, []);

    const handleRename = useCallback(
        (pane: Pane, entry: FileEntry) => {
            const paneState = pane === "left" ? left : right;
            const { mountId } = paneState;
            if (!mountId) return;

            void (async () => {
                const newName = await modal.prompt("New name:", { defaultValue: entry.name });
                if (newName === null || newName.trim() === "" || newName.trim() === entry.name) return;

                const parentDir = entry.path.lastIndexOf("/") > 0
                    ? entry.path.substring(0, entry.path.lastIndexOf("/"))
                    : "";

                const srcPath = entry.path.replace(/^\//, "");
                const destPath = parentDir
                    ? `${parentDir.replace(/^\//, "")}/${newName.trim()}`
                    : newName.trim();

                const srcUri = buildFileUri(mountId, srcPath);
                const destUri = buildFileUri(mountId, destPath);

                try {
                    const result = await moveFile(srcUri, destUri);
                    if (result.operationId) {
                        startOperation(result.operationId);
                    } else {
                        refreshPane(pane);
                    }
                } catch (err) {
                    showError(`Rename failed: ${err instanceof Error ? err.message : String(err)}`);
                }
            })();
        },
        [left, right, mounts.mounts, modal, refreshPane, startOperation, showError],
    );

    const handleFileDrop = useCallback(
        (destSide: Pane) =>
            (dragged: DraggedFile) => {
                const dest = destSide === "left" ? left : right;
                if (!dest.mountId) return;

                // No-op: same mount and same directory
                const srcDir = dragged.path.replace(/\/[^/]+$/, "") || "/";
                if (dragged.mountId === dest.mountId && srcDir === dest.path) return;

                const srcPath = dragged.path.replace(/^\//, "");
                const destDir = dest.path === "/" ? "" : dest.path.replace(/^\//, "");
                const destPath = destDir ? `${destDir}/${dragged.name}` : dragged.name;

                const srcUri = buildFileUri(dragged.mountId, srcPath);
                const destUri = buildFileUri(dest.mountId!, destPath);

                void (async () => {
                    try {
                        const result = await moveFile(srcUri, destUri);
                        if (result.operationId) {
                            startOperation(result.operationId);
                        } else {
                            refreshBoth();
                        }
                    } catch (err: unknown) {
                        showError(`Move failed: ${err instanceof Error ? err.message : String(err)}`);
                    }
                })();
            },
        [left, right, mounts.mounts, refreshBoth, startOperation],
    );

    const buildFileContextMenu = useCallback(
        (pane: Pane, entry: FileEntry): MenuItem[] => {
            const paneState = pane === "left" ? left : right;
            const { mountId } = paneState;
            if (!mountId) return [];

            const items: MenuItem[] = [];

            if (entry.type === "file") {
                items.push({
                    label: "Download",
                    onClick: () => downloadFile(mountId, entry.path, entry.name),
                });
            }

            items.push({
                label: "Copy",
                onClick: () => setClipboard({ operation: "copy", mountId, path: entry.path, name: entry.name }),
            });

            items.push({
                label: "Cut",
                onClick: () => setClipboard({ operation: "cut", mountId, path: entry.path, name: entry.name }),
            });

            items.push({
                label: "Rename",
                onClick: () => handleRename(pane, entry),
            });

            items.push({
                label: "Delete",
                onClick: () => {
                    void (async () => {
                        const ok = await modal.confirm(`Delete "${entry.name}"?`, { confirmText: "Delete" });
                        if (!ok) return;
                        try {
                            const result = await deleteFile(mountId, entry.path);
                            if (result.operationId) {
                                startOperation(result.operationId);
                            } else {
                                refreshPane(pane);
                            }
                        } catch (err) {
                            showError(`Delete failed: ${err instanceof Error ? err.message : String(err)}`);
                        }
                    })();
                },
            });

            return items;
        },
        [left, right, refreshPane, handleRename, modal, startOperation, showError],
    );

    const buildEmptyContextMenu = useCallback(
        (pane: Pane): MenuItem[] => {
            if (!clipboard) return [];

            const paneState = pane === "left" ? left : right;
            const { mountId, path } = paneState;
            if (!mountId) return [];

            return [
                {
                    label: "Paste",
                    onClick: () => {
                        const srcPath = clipboard.path.replace(/^\//, "");
                        const srcUri = buildFileUri(clipboard.mountId, srcPath);

                        const destDir = path === "/" ? "" : path.replace(/^\//, "");
                        const destPath = destDir ? `${destDir}/${clipboard.name}` : clipboard.name;
                        const destUri = buildFileUri(mountId, destPath);

                        const op = clipboard.operation;
                        void (async () => {
                            try {
                                if (op === "cut") {
                                    const result = await moveFile(srcUri, destUri);
                                    setClipboard(null);
                                    if (result.operationId) {
                                        startOperation(result.operationId);
                                    } else {
                                        refreshBoth();
                                    }
                                } else {
                                    const result = await copyFile(srcUri, destUri);
                                    startOperation(result.operationId);
                                }
                            } catch (err) {
                                showError(`Paste failed: ${err instanceof Error ? err.message : String(err)}`);
                            }
                        })();
                    },
                },
            ];
        },
        [clipboard, left, right, mounts.mounts, refreshBoth, startOperation],
    );

    const handleFileContextMenu = useCallback(
        (pane: Pane, e: React.MouseEvent, entry: FileEntry) => {
            e.preventDefault();
            e.stopPropagation();
            const items = buildFileContextMenu(pane, entry);
            if (items.length === 0) return;
            setContextMenu({ x: e.clientX, y: e.clientY, items });
        },
        [buildFileContextMenu],
    );

    const handleEmptyContextMenu = useCallback(
        (pane: Pane, e: React.MouseEvent) => {
            e.preventDefault();
            const items = buildEmptyContextMenu(pane);
            if (items.length === 0) return;
            setContextMenu({ x: e.clientX, y: e.clientY, items });
        },
        [buildEmptyContextMenu],
    );

    return (
        <div className={styles.app}>
            <div className={styles.tabBar}>
                <button
                    className={`${styles.tab} ${activeTab === "commander" ? styles.tabActive : ""}`}
                    onClick={() => setActiveTab("commander")}
                >
                    Commander
                </button>
                <button
                    className={`${styles.tab} ${activeTab === "mounts" ? styles.tabActive : ""}`}
                    onClick={() => setActiveTab("mounts")}
                >
                    Mounts
                </button>
                <button
                    className={`${styles.tab} ${activeTab === "queue" ? styles.tabActive : ""}`}
                    onClick={() => setActiveTab("queue")}
                >
                    Queue
                </button>
            </div>

            {activeTab === "commander" ? (
                <div className={styles.commander}>
                    <div className={styles.paneLeft}>
                        <FileBrowser
                            mountId={left.mountId}
                            path={left.path}
                            mounts={mounts.mounts}
                            onMountChange={(mountId) => setLeft({ mountId, path: "/" })}
                            onNavigate={(path) => setLeft((prev) => ({ ...prev, path }))}
                            refreshKey={leftRefreshKey}
                            onFileDrop={handleFileDrop("left")}
                            onFileContextMenu={(e, entry) => handleFileContextMenu("left", e, entry)}
                            onEmptyContextMenu={(e) => handleEmptyContextMenu("left", e)}
                            onRename={(entry) => handleRename("left", entry)}
                            cutPath={clipboard?.operation === "cut" ? clipboard.path : null}
                            onOperationStarted={startOperation}
                        />
                    </div>
                    <div className={styles.paneRight}>
                        <FileBrowser
                            mountId={right.mountId}
                            path={right.path}
                            mounts={mounts.mounts}
                            onMountChange={(mountId) => setRight({ mountId, path: "/" })}
                            onNavigate={(path) => setRight((prev) => ({ ...prev, path }))}
                            refreshKey={rightRefreshKey}
                            onFileDrop={handleFileDrop("right")}
                            onFileContextMenu={(e, entry) => handleFileContextMenu("right", e, entry)}
                            onEmptyContextMenu={(e) => handleEmptyContextMenu("right", e)}
                            onRename={(entry) => handleRename("right", entry)}
                            cutPath={clipboard?.operation === "cut" ? clipboard.path : null}
                            onOperationStarted={startOperation}
                        />
                    </div>
                </div>
            ) : activeTab === "mounts" ? (
                <div className={styles.mountsTabWrapper}>
                    <MountsTab mounts={mounts} />
                </div>
            ) : (
                <div className={styles.mountsTabWrapper}>
                    <QueueTab />
                </div>
            )}

            {contextMenu && (
                <ContextMenu
                    x={contextMenu.x}
                    y={contextMenu.y}
                    items={contextMenu.items}
                    onClose={closeContextMenu}
                />
            )}

            {progressOperationId !== null && (
                <ProgressModal
                    operationId={progressOperationId}
                    onDismiss={() => setProgressOperationId(null)}
                />
            )}

            <StatusIndicator onClick={() => setActiveTab("queue")} />
        </div>
    );
}
