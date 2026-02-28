import { useState, useCallback } from "react";
import { useMounts } from "./hooks/useMounts.js";
import { FileBrowser } from "./components/FileBrowser.js";
import type { DraggedFile } from "./components/FileBrowser.js";
import { MountsTab } from "./components/MountsTab.js";
import { ContextMenu } from "./components/ContextMenu.js";
import type { MenuItem } from "./components/ContextMenu.js";
import type { FileEntry } from "@file-manager/schemas";
import { moveFile, copyFile, deleteFile, downloadFile } from "./api/client.js";
import { useToast } from "./hooks/useToast.js";
import styles from "./App.module.css";

type Tab = "commander" | "mounts";
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
    const [activeTab, setActiveTab] = useState<Tab>("commander");
    const [left, setLeft] = useState<PaneState>({ mountId: null, path: "/" });
    const [right, setRight] = useState<PaneState>({ mountId: null, path: "/" });
    const [leftRefreshKey, setLeftRefreshKey] = useState(0);
    const [rightRefreshKey, setRightRefreshKey] = useState(0);
    const [clipboard, setClipboard] = useState<ClipboardEntry | null>(null);
    const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);

    const closeContextMenu = useCallback(() => setContextMenu(null), []);

    const refreshPane = useCallback((pane: Pane) => {
        if (pane === "left") setLeftRefreshKey((k) => k + 1);
        else setRightRefreshKey((k) => k + 1);
    }, []);

    const refreshBoth = useCallback(() => {
        setLeftRefreshKey((k) => k + 1);
        setRightRefreshKey((k) => k + 1);
    }, []);

    const handleFileDrop = useCallback(
        (destSide: Pane) =>
            (dragged: DraggedFile) => {
                const dest = destSide === "left" ? left : right;
                if (!dest.mountId) return;

                const srcMount = mounts.mounts.find((m) => m.mountId === dragged.mountId);
                const destMount = mounts.mounts.find((m) => m.mountId === dest.mountId);
                if (!srcMount || !destMount) return;

                // No-op: same mount and same directory
                const srcDir = dragged.path.replace(/\/[^/]+$/, "") || "/";
                if (dragged.mountId === dest.mountId && srcDir === dest.path) return;

                const srcPath = dragged.path.replace(/^\//, "");
                const destDir = dest.path === "/" ? "" : dest.path.replace(/^\//, "");
                const destPath = destDir ? `${destDir}/${dragged.name}` : dragged.name;

                const srcUri = `${srcMount.scheme}://${dragged.mountId}/${srcPath}`;
                const destUri = `${destMount.scheme}://${dest.mountId}/${destPath}`;

                void (async () => {
                    try {
                        await moveFile(srcUri, destUri);
                        refreshBoth();
                    } catch (err: unknown) {
                        showError(`Move failed: ${err instanceof Error ? err.message : String(err)}`);
                    }
                })();
            },
        [left, right, mounts.mounts, refreshBoth],
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
                label: "Delete",
                onClick: () => {
                    void (async () => {
                        try {
                            await deleteFile(mountId, entry.path);
                            refreshPane(pane);
                        } catch (err) {
                            showError(`Delete failed: ${err instanceof Error ? err.message : String(err)}`);
                        }
                    })();
                },
            });

            return items;
        },
        [left, right, refreshPane],
    );

    const buildEmptyContextMenu = useCallback(
        (pane: Pane): MenuItem[] => {
            if (!clipboard) return [];

            const paneState = pane === "left" ? left : right;
            const { mountId, path } = paneState;
            if (!mountId) return [];

            const destMount = mounts.mounts.find((m) => m.mountId === mountId);
            if (!destMount) return [];

            return [
                {
                    label: "Paste",
                    onClick: () => {
                        const srcMount = mounts.mounts.find((m) => m.mountId === clipboard.mountId);
                        if (!srcMount) return;

                        const srcPath = clipboard.path.replace(/^\//, "");
                        const srcUri = `${srcMount.scheme}://${clipboard.mountId}/${srcPath}`;

                        const destDir = path === "/" ? "" : path.replace(/^\//, "");
                        const destPath = destDir ? `${destDir}/${clipboard.name}` : clipboard.name;
                        const destUri = `${destMount.scheme}://${mountId}/${destPath}`;

                        const op = clipboard.operation;
                        void (async () => {
                            try {
                                if (op === "cut") {
                                    await moveFile(srcUri, destUri);
                                    setClipboard(null);
                                } else {
                                    await copyFile(srcUri, destUri);
                                }
                                refreshBoth();
                            } catch (err) {
                                showError(`Paste failed: ${err instanceof Error ? err.message : String(err)}`);
                            }
                        })();
                    },
                },
            ];
        },
        [clipboard, left, right, mounts.mounts, refreshBoth],
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
                            cutPath={clipboard?.operation === "cut" ? clipboard.path : null}
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
                            cutPath={clipboard?.operation === "cut" ? clipboard.path : null}
                        />
                    </div>
                </div>
            ) : (
                <div className={styles.mountsTabWrapper}>
                    <MountsTab mounts={mounts} />
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


        </div>
    );
}
