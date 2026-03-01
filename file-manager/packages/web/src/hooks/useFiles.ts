import { useState, useEffect, useCallback } from "react";
import type { FileEntry } from "@file-manager/schemas";
import { listFiles, uploadFile, deleteFile, makeDirectory } from "../api/client.js";

export interface UseFiles {
    files: FileEntry[];
    loading: boolean;
    error: string | null;
    upload: (file: File) => Promise<string | undefined>;
    remove: (path: string) => Promise<string | undefined>;
    mkdir: (dirPath: string) => Promise<void>;
    refresh: () => void;
}

export function useFiles(mountId: string | null, path: string, externalKey = 0): UseFiles {
    const [files, setFiles] = useState<FileEntry[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [tick, setTick] = useState(0);

    useEffect(() => {
        if (!mountId) {
            setFiles([]);
            return;
        }
        let cancelled = false;
        setLoading(true);
        listFiles(mountId, path)
            .then((data) => {
                if (!cancelled) {
                    setFiles(data);
                    setError(null);
                }
            })
            .catch((err: unknown) => {
                if (!cancelled) {
                    setError(err instanceof Error ? err.message : String(err));
                    setFiles([]);
                }
            })
            .finally(() => {
                if (!cancelled) setLoading(false);
            });
        return () => {
            cancelled = true;
        };
    }, [mountId, path, tick, externalKey]);

    const refresh = useCallback(() => setTick((t) => t + 1), []);

    const upload = useCallback(
        async (file: File): Promise<string | undefined> => {
            if (!mountId) return undefined;
            const dest = path === "/" ? `/${file.name}` : `${path}/${file.name}`;
            const result = await uploadFile(mountId, dest, file);
            refresh();
            return result.operationId;
        },
        [mountId, path, refresh],
    );

    const remove = useCallback(
        async (filePath: string): Promise<string | undefined> => {
            if (!mountId) return undefined;
            const result = await deleteFile(mountId, filePath);
            refresh();
            return result.operationId;
        },
        [mountId, refresh],
    );

    const mkdir = useCallback(
        async (dirPath: string) => {
            if (!mountId) return;
            await makeDirectory(mountId, dirPath);
            refresh();
        },
        [mountId, refresh],
    );

    return { files, loading, error, upload, remove, mkdir, refresh };
}
