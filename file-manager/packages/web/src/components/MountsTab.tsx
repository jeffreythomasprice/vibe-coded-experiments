import { useState } from "react";
import type { MountInfo } from "@file-manager/schemas";
import type { UseMounts } from "../hooks/useMounts.js";
import { useToast } from "../hooks/useToast.js";
import styles from "./MountsTab.module.css";

interface Props {
    mounts: UseMounts;
}

export function MountsTab({ mounts }: Props) {
    const { showError } = useToast();
    const [showForm, setShowForm] = useState(false);
    const [mountId, setMountId] = useState("");
    const [rootDir, setRootDir] = useState("");
    const [submitting, setSubmitting] = useState(false);

    async function handleAdd(e: React.FormEvent) {
        e.preventDefault();
        if (!mountId.trim() || !rootDir.trim()) return;
        setSubmitting(true);
        const info: MountInfo = {
            mountId: mountId.trim(),
            scheme: "local",
            config: { rootDir: rootDir.trim() },
        };
        try {
            await mounts.add(info);
            setMountId("");
            setRootDir("");
            setShowForm(false);
        } catch (err) {
            showError(`Add mount failed: ${err instanceof Error ? err.message : String(err)}`);
        } finally {
            setSubmitting(false);
        }
    }

    return (
        <div className={styles.container}>
            <h2 className={styles.heading}>Mounts</h2>

            {mounts.error && <p className={styles.error}>{mounts.error}</p>}

            <table className={styles.table}>
                <thead>
                    <tr>
                        <th>Mount ID</th>
                        <th>Scheme</th>
                        <th>Root Dir</th>
                        <th>Actions</th>
                    </tr>
                </thead>
                <tbody>
                    {mounts.mounts.length === 0 ? (
                        <tr>
                            <td colSpan={4} className={styles.emptyCell}>
                                No mounts registered.
                            </td>
                        </tr>
                    ) : (
                        mounts.mounts.map((m) => (
                            <tr key={m.mountId} className={styles.row}>
                                <td className={styles.cell}>{m.mountId}</td>
                                <td className={styles.cell}>{m.scheme}</td>
                                <td className={styles.cell}>
                                    {(m.config as Record<string, string>)["rootDir"] ?? "—"}
                                </td>
                                <td className={styles.cell}>
                                    <button
                                        className={styles.removeBtn}
                                        onClick={() => {
                                            void (async () => {
                                                try {
                                                    await mounts.remove(m.mountId);
                                                } catch (err) {
                                                    showError(`Remove mount failed: ${err instanceof Error ? err.message : String(err)}`);
                                                }
                                            })();
                                        }}
                                    >
                                        Remove
                                    </button>
                                </td>
                            </tr>
                        ))
                    )}
                </tbody>
            </table>

            {showForm ? (
                <form
                    className={styles.form}
                    onSubmit={(e) => {
                        void handleAdd(e);
                    }}
                >
                    <h3 className={styles.formHeading}>Add Mount</h3>
                    <div className={styles.formRow}>
                        <label className={styles.label}>Mount ID</label>
                        <input
                            className={styles.input}
                            placeholder="e.g. docs"
                            value={mountId}
                            onChange={(e) => setMountId(e.target.value)}
                            required
                        />
                    </div>
                    <div className={styles.formRow}>
                        <label className={styles.label}>Root Directory</label>
                        <input
                            className={styles.input}
                            placeholder="Absolute path on server"
                            value={rootDir}
                            onChange={(e) => setRootDir(e.target.value)}
                            required
                        />
                    </div>
                    <div className={styles.formActions}>
                        <button className={styles.addBtn} type="submit" disabled={submitting}>
                            {submitting ? "Adding…" : "Add"}
                        </button>
                        <button
                            type="button"
                            className={styles.cancelBtn}
                            onClick={() => {
                                setShowForm(false);
                            }}
                        >
                            Cancel
                        </button>
                    </div>
                </form>
            ) : (
                <button className={styles.newMountBtn} onClick={() => setShowForm(true)}>
                    + Add Mount
                </button>
            )}
        </div>
    );
}
