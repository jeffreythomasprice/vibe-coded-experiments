import { createContext, useContext, useState, useCallback, useRef, useEffect, createElement } from "react";
import type { ReactNode } from "react";
import type { Operation } from "@file-manager/schemas";
import { connectOperationEvents } from "../api/client.js";
import { useToast } from "./useToast.js";

interface OperationsContextValue {
    operations: Map<string, Operation>;
    activeCount: number;
    trackOperation: (id: string) => void;
    onOperationComplete: (callback: () => void) => () => void;
}

const OperationsContext = createContext<OperationsContextValue | null>(null);

export function OperationsProvider({ children }: { children: ReactNode }) {
    const [operations, setOperations] = useState<Map<string, Operation>>(new Map());
    const completionCallbacksRef = useRef<Set<() => void>>(new Set());
    const trackedIdsRef = useRef<Set<string>>(new Set());
    const { showError } = useToast();
    const showErrorRef = useRef(showError);
    showErrorRef.current = showError;

    useEffect(() => {
        let es: EventSource | null = null;
        let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

        function connect() {
            es = connectOperationEvents({
                onCreated(op) {
                    setOperations((prev) => {
                        const next = new Map(prev);
                        next.set(op.id, op);
                        return next;
                    });
                },
                onProgress(op) {
                    setOperations((prev) => {
                        const next = new Map(prev);
                        next.set(op.id, op);
                        return next;
                    });
                },
                onCompleted(op) {
                    setOperations((prev) => {
                        const next = new Map(prev);
                        next.set(op.id, op);
                        return next;
                    });
                    for (const cb of completionCallbacksRef.current) {
                        cb();
                    }
                },
                onFailed(op) {
                    setOperations((prev) => {
                        const next = new Map(prev);
                        next.set(op.id, op);
                        return next;
                    });
                    showErrorRef.current(`Operation failed: ${op.error ?? "Unknown error"}`);
                    for (const cb of completionCallbacksRef.current) {
                        cb();
                    }
                },
            });

            es.onerror = () => {
                es?.close();
                reconnectTimer = setTimeout(connect, 3000);
            };
        }

        connect();

        return () => {
            es?.close();
            if (reconnectTimer !== null) clearTimeout(reconnectTimer);
        };
    }, []);

    const activeCount = Array.from(operations.values()).filter(
        (op) => op.status === "pending" || op.status === "in_progress",
    ).length;

    const trackOperation = useCallback((id: string) => {
        trackedIdsRef.current.add(id);
    }, []);

    const onOperationComplete = useCallback((callback: () => void): (() => void) => {
        completionCallbacksRef.current.add(callback);
        return () => {
            completionCallbacksRef.current.delete(callback);
        };
    }, []);

    return createElement(
        OperationsContext.Provider,
        { value: { operations, activeCount, trackOperation, onOperationComplete } },
        children,
    );
}

export function useOperations(): OperationsContextValue {
    const ctx = useContext(OperationsContext);
    if (!ctx) throw new Error("useOperations must be used within OperationsProvider");
    return ctx;
}
