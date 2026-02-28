import { useToast } from "../hooks/useToast.js";
import styles from "./Toaster.module.css";

export function Toaster() {
    const { toasts, dismiss } = useToast();

    if (toasts.length === 0) return null;

    return (
        <div className={styles.container}>
            {toasts.map((toast) => (
                <div
                    key={toast.id}
                    className={`${styles.toast} ${toast.type === "error" ? styles.error : styles.info}`}
                    onClick={() => dismiss(toast.id)}
                >
                    {toast.message}
                </div>
            ))}
        </div>
    );
}
