import { useState, useCallback } from "react";
import { useMounts } from "./hooks/useMounts.js";
import { FileBrowser } from "./components/FileBrowser.js";
import type { DraggedFile } from "./components/FileBrowser.js";
import { MountsTab } from "./components/MountsTab.js";
import { moveFile } from "./api/client.js";
import styles from "./App.module.css";

type Tab = "commander" | "mounts";

interface PaneState {
    mountId: string | null;
    path: string;
}

export function App() {
    const mounts = useMounts();
    const [activeTab, setActiveTab] = useState<Tab>("commander");
    const [left, setLeft] = useState<PaneState>({ mountId: null, path: "/" });
    const [right, setRight] = useState<PaneState>({ mountId: null, path: "/" });
    const [leftRefreshKey, setLeftRefreshKey] = useState(0);
    const [rightRefreshKey, setRightRefreshKey] = useState(0);

    const handleFileDrop = useCallback(
        (destSide: "left" | "right") =>
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
                        setLeftRefreshKey((k) => k + 1);
                        setRightRefreshKey((k) => k + 1);
                    } catch (err: unknown) {
                        console.error("Move failed:", err);
                    }
                })();
            },
        [left, right, mounts.mounts],
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
                        />
                    </div>
                </div>
            ) : (
                <div className={styles.mountsTabWrapper}>
                    <MountsTab mounts={mounts} />
                </div>
            )}
        </div>
    );
}
