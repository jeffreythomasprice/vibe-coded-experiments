import styles from "./ProgressBar.module.css";

interface Props {
    processedBytes: number;
    totalBytes: number;
    processedFiles: number;
    totalFiles: number;
    currentFile?: string;
}

export function ProgressBar({ processedBytes, totalBytes, processedFiles, totalFiles, currentFile }: Props) {
    const pct = totalBytes > 0 ? Math.round((processedBytes / totalBytes) * 100) : 0;

    return (
        <div className={styles.container}>
            <div className={styles.track}>
                <div className={styles.fill} style={{ width: `${pct}%` }} />
            </div>
            <div className={styles.info}>
                <span className={styles.files}>{processedFiles}/{totalFiles} files</span>
                <span className={styles.pct}>{pct}%</span>
            </div>
            {currentFile && (
                <div className={styles.currentFile} title={currentFile}>
                    {currentFile}
                </div>
            )}
        </div>
    );
}
