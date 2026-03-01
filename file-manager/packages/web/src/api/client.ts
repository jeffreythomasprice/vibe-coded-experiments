import type { FileEntry, MountInfo, Operation } from "@file-manager/schemas";

const BASE_URL = (import.meta.env["VITE_API_URL"] as string | undefined) ?? "";

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
    const url = `${BASE_URL}${path}`;
    const headers: Record<string, string> = {};
    if (options.body !== undefined && options.body !== null) {
        headers["Content-Type"] = "application/json";
    }
    const res = await fetch(url, {
        headers: {
            ...headers,
            ...(options.headers as Record<string, string>),
        },
        ...options,
    });

    if (!res.ok) {
        let message = `HTTP ${res.status}`;
        try {
            const body = (await res.json()) as { error?: string };
            message = body.error ?? message;
        } catch {
            // ignore parse errors
        }
        throw new Error(message);
    }

    if (res.status === 204) return undefined as T;

    return res.json() as Promise<T>;
}

export function buildFileUri(mountId: string, path: string): string {
    return `${mountId}://${path}`;
}

// ── Mounts ────────────────────────────────────────────────────────────────────

export async function getMounts(): Promise<MountInfo[]> {
    return request<MountInfo[]>("/api/v1/providers");
}

export async function addMount(info: MountInfo): Promise<MountInfo> {
    return request<MountInfo>("/api/v1/providers", {
        method: "POST",
        body: JSON.stringify(info),
    });
}

export async function removeMount(mountId: string): Promise<void> {
    return request<void>(`/api/v1/providers/${mountId}`, { method: "DELETE" });
}

// ── Files ─────────────────────────────────────────────────────────────────────

export async function listFiles(mountId: string, path: string): Promise<FileEntry[]> {
    const p = path.replace(/^\//, "");
    return request<FileEntry[]>(`/api/v1/files/${mountId}/${p}`);
}

export async function uploadFile(mountId: string, path: string, file: File): Promise<{ path: string; operationId: string }> {
    const p = path.replace(/^\//, "");
    const url = `${BASE_URL}/api/v1/files/${mountId}/${p}`;
    const res = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/octet-stream" },
        body: file,
    });
    if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
    }
    return res.json() as Promise<{ path: string; operationId: string }>;
}

export function downloadFile(mountId: string, path: string, filename: string): void {
    const p = path.replace(/^\//, "");
    const url = `${BASE_URL}/api/v1/files/${mountId}/${p}`;
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
}

export async function deleteFile(mountId: string, path: string): Promise<{ operationId: string }> {
    const p = path.replace(/^\//, "");
    return request<{ operationId: string }>(`/api/v1/files/${mountId}/${p}`, { method: "DELETE" });
}

export async function makeDirectory(mountId: string, path: string): Promise<void> {
    const p = path.replace(/^\//, "");
    await request<{ path: string }>(`/api/v1/files/${mountId}/${p}`, { method: "PUT" });
}

export async function moveFile(src: string, dest: string): Promise<{ src: string; dest: string; operationId?: string }> {
    return request<{ src: string; dest: string; operationId?: string }>("/api/v1/files/move", {
        method: "POST",
        body: JSON.stringify({ src, dest }),
    });
}

export async function copyFile(src: string, dest: string): Promise<{ src: string; dest: string; operationId: string }> {
    return request<{ src: string; dest: string; operationId: string }>("/api/v1/files/copy", {
        method: "POST",
        body: JSON.stringify({ src, dest }),
    });
}

// ── Operations ───────────────────────────────────────────────────────────────

export async function getOperations(status?: string): Promise<Operation[]> {
    const qs = status ? `?status=${encodeURIComponent(status)}` : "";
    return request<Operation[]>(`/api/v1/operations${qs}`);
}

export interface OperationEventHandlers {
    onCreated?: (op: Operation) => void;
    onProgress?: (op: Operation) => void;
    onCompleted?: (op: Operation) => void;
    onFailed?: (op: Operation) => void;
}

export function connectOperationEvents(handlers: OperationEventHandlers): EventSource {
    const url = `${BASE_URL}/api/v1/operations/events`;
    const es = new EventSource(url);

    const parse = (e: MessageEvent) => JSON.parse(e.data as string) as Operation;

    if (handlers.onCreated) {
        const h = handlers.onCreated;
        es.addEventListener("created", (e) => h(parse(e)));
    }
    if (handlers.onProgress) {
        const h = handlers.onProgress;
        es.addEventListener("progress", (e) => h(parse(e)));
    }
    if (handlers.onCompleted) {
        const h = handlers.onCompleted;
        es.addEventListener("completed", (e) => h(parse(e)));
    }
    if (handlers.onFailed) {
        const h = handlers.onFailed;
        es.addEventListener("failed", (e) => h(parse(e)));
    }

    return es;
}
