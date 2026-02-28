import { createContext, useContext, useState, useCallback, useRef, useEffect, createElement } from "react";
import type { ReactNode } from "react";

interface Toast {
    id: number;
    message: string;
    type: "error" | "info";
}

interface ToastContextValue {
    toasts: Toast[];
    showError: (message: string) => void;
    showInfo: (message: string) => void;
    dismiss: (id: number) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
    const [toasts, setToasts] = useState<Toast[]>([]);
    const nextIdRef = useRef(0);
    const timersRef = useRef(new Map<number, ReturnType<typeof setTimeout>>());

    useEffect(() => {
        return () => {
            timersRef.current.forEach((t) => clearTimeout(t));
        };
    }, []);

    const addToast = useCallback((message: string, type: Toast["type"]) => {
        const id = nextIdRef.current++;
        setToasts((prev) => [...prev, { id, message, type }]);
        const timer = setTimeout(() => {
            timersRef.current.delete(id);
            setToasts((prev) => prev.filter((t) => t.id !== id));
        }, 4000);
        timersRef.current.set(id, timer);
    }, []);

    const showError = useCallback((message: string) => addToast(message, "error"), [addToast]);
    const showInfo = useCallback((message: string) => addToast(message, "info"), [addToast]);

    const dismiss = useCallback((id: number) => {
        const timer = timersRef.current.get(id);
        if (timer !== undefined) {
            clearTimeout(timer);
            timersRef.current.delete(id);
        }
        setToasts((prev) => prev.filter((t) => t.id !== id));
    }, []);

    return createElement(ToastContext.Provider, { value: { toasts, showError, showInfo, dismiss } }, children);
}

export function useToast(): ToastContextValue {
    const ctx = useContext(ToastContext);
    if (!ctx) throw new Error("useToast must be used within ToastProvider");
    return ctx;
}
