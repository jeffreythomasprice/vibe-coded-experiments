import type { FileEntry, MountInfo } from "@file-manager/schemas";

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

export async function uploadFile(mountId: string, path: string, file: File): Promise<void> {
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
}

export function downloadFile(mountId: string, path: string, filename: string): void {
    const p = path.replace(/^\//, "");
    const url = `${BASE_URL}/api/v1/files/${mountId}/${p}`;
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
}

export async function deleteFile(mountId: string, path: string): Promise<void> {
    const p = path.replace(/^\//, "");
    return request<void>(`/api/v1/files/${mountId}/${p}`, { method: "DELETE" });
}

export async function makeDirectory(mountId: string, path: string): Promise<void> {
    const p = path.replace(/^\//, "");
    await request<{ path: string }>(`/api/v1/files/${mountId}/${p}`, { method: "PUT" });
}
