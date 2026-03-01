import { useState } from "react";
import type { MountInfo } from "@file-manager/schemas";
import type { UseMounts } from "../hooks/useMounts.js";
import { useToast } from "../hooks/useToast.js";
import styles from "./MountsTab.module.css";

type Scheme = "local" | "s3";

function formatConfig(mount: MountInfo): string {
    const c = mount.config as Record<string, string>;
    if (mount.scheme === "local") return c["rootDir"] ?? "—";
    if (mount.scheme === "s3") {
        const parts = [c["bucket"]];
        if (c["prefix"]) parts.push(c["prefix"]);
        return parts.join("/");
    }
    return JSON.stringify(mount.config);
}

interface Props {
    mounts: UseMounts;
}

export function MountsTab({ mounts }: Props) {
    const { showError } = useToast();
    const [showForm, setShowForm] = useState(false);
    const [mountId, setMountId] = useState("");
    const [scheme, setScheme] = useState<Scheme>("local");
    const [submitting, setSubmitting] = useState(false);

    // Local fields
    const [rootDir, setRootDir] = useState("");

    // S3 fields
    const [bucket, setBucket] = useState("");
    const [prefix, setPrefix] = useState("");
    const [region, setRegion] = useState("");
    const [accessKeyId, setAccessKeyId] = useState("");
    const [secretAccessKey, setSecretAccessKey] = useState("");

    function resetForm() {
        setMountId("");
        setScheme("local");
        setRootDir("");
        setBucket("");
        setPrefix("");
        setRegion("");
        setAccessKeyId("");
        setSecretAccessKey("");
    }

    function buildConfig(): Record<string, string> {
        if (scheme === "local") {
            return { rootDir: rootDir.trim() };
        }
        const config: Record<string, string> = { bucket: bucket.trim() };
        if (prefix.trim()) config["prefix"] = prefix.trim();
        if (region.trim()) config["region"] = region.trim();
        if (accessKeyId.trim()) config["accessKeyId"] = accessKeyId.trim();
        if (secretAccessKey.trim()) config["secretAccessKey"] = secretAccessKey.trim();
        return config;
    }

    function isFormValid(): boolean {
        if (!mountId.trim()) return false;
        if (scheme === "local") return rootDir.trim().length > 0;
        if (scheme === "s3") return bucket.trim().length > 0;
        return false;
    }

    async function handleAdd(e: React.FormEvent) {
        e.preventDefault();
        if (!isFormValid()) return;
        setSubmitting(true);
        const info: MountInfo = {
            mountId: mountId.trim(),
            scheme,
            config: buildConfig(),
        };
        try {
            await mounts.add(info);
            resetForm();
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
                        <th>Config</th>
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
                                <td className={styles.cell}>{formatConfig(m)}</td>
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
                        <label className={styles.label}>Scheme</label>
                        <select
                            className={styles.input}
                            value={scheme}
                            onChange={(e) => setScheme(e.target.value as Scheme)}
                        >
                            <option value="local">Local</option>
                            <option value="s3">S3</option>
                        </select>
                    </div>

                    {scheme === "local" && (
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
                    )}

                    {scheme === "s3" && (
                        <>
                            <div className={styles.formRow}>
                                <label className={styles.label}>Bucket</label>
                                <input
                                    className={styles.input}
                                    placeholder="my-bucket"
                                    value={bucket}
                                    onChange={(e) => setBucket(e.target.value)}
                                    required
                                />
                            </div>
                            <div className={styles.formRow}>
                                <label className={styles.label}>Prefix</label>
                                <input
                                    className={styles.input}
                                    placeholder="optional/key/prefix"
                                    value={prefix}
                                    onChange={(e) => setPrefix(e.target.value)}
                                />
                            </div>
                            <div className={styles.formRow}>
                                <label className={styles.label}>Region</label>
                                <input
                                    className={styles.input}
                                    placeholder="e.g. us-east-1"
                                    value={region}
                                    onChange={(e) => setRegion(e.target.value)}
                                />
                            </div>
                            <div className={styles.formRow}>
                                <label className={styles.label}>Access Key ID</label>
                                <input
                                    className={styles.input}
                                    placeholder="Optional (uses env/IAM if omitted)"
                                    value={accessKeyId}
                                    onChange={(e) => setAccessKeyId(e.target.value)}
                                />
                            </div>
                            <div className={styles.formRow}>
                                <label className={styles.label}>Secret Access Key</label>
                                <input
                                    className={styles.input}
                                    type="password"
                                    placeholder="Optional (uses env/IAM if omitted)"
                                    value={secretAccessKey}
                                    onChange={(e) => setSecretAccessKey(e.target.value)}
                                />
                            </div>
                        </>
                    )}

                    <div className={styles.formActions}>
                        <button className={styles.addBtn} type="submit" disabled={submitting || !isFormValid()}>
                            {submitting ? "Adding…" : "Add"}
                        </button>
                        <button
                            type="button"
                            className={styles.cancelBtn}
                            onClick={() => {
                                resetForm();
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
