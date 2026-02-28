import { useState, useEffect, useCallback } from "react";
import type { MountInfo } from "@file-manager/schemas";
import { getMounts, addMount, removeMount } from "../api/client.js";

export interface UseMounts {
    mounts: MountInfo[];
    loading: boolean;
    error: string | null;
    add: (info: MountInfo) => Promise<void>;
    remove: (mountId: string) => Promise<void>;
    refresh: () => void;
}

export function useMounts(): UseMounts {
    const [mounts, setMounts] = useState<MountInfo[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [tick, setTick] = useState(0);

    useEffect(() => {
        let cancelled = false;
        setLoading(true);
        getMounts()
            .then((data) => {
                if (!cancelled) {
                    setMounts(data);
                    setError(null);
                }
            })
            .catch((err: unknown) => {
                if (!cancelled) setError(err instanceof Error ? err.message : String(err));
            })
            .finally(() => {
                if (!cancelled) setLoading(false);
            });
        return () => {
            cancelled = true;
        };
    }, [tick]);

    const refresh = useCallback(() => setTick((t) => t + 1), []);

    const add = useCallback(
        async (info: MountInfo) => {
            await addMount(info);
            refresh();
        },
        [refresh],
    );

    const remove = useCallback(
        async (mountId: string) => {
            await removeMount(mountId);
            refresh();
        },
        [refresh],
    );

    return { mounts, loading, error, add, remove, refresh };
}
