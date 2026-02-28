import { useState } from "react";
import { useMounts } from "./hooks/useMounts.js";
import { FileBrowser } from "./components/FileBrowser.js";
import { MountsTab } from "./components/MountsTab.js";
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
                        />
                    </div>
                    <div className={styles.paneRight}>
                        <FileBrowser
                            mountId={right.mountId}
                            path={right.path}
                            mounts={mounts.mounts}
                            onMountChange={(mountId) => setRight({ mountId, path: "/" })}
                            onNavigate={(path) => setRight((prev) => ({ ...prev, path }))}
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
