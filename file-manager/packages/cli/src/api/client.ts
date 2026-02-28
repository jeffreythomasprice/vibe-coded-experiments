import type { FileEntry, FileStat } from "@file-manager/shared";
import { loadConfig } from "../config.js";

const _cfg = loadConfig();
const BASE_URL = (process.env["FILE_MANAGER_API_URL"] ?? _cfg.apiUrl ?? "http://localhost:8000").replace(/\/$/, "");

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
    const url = `${BASE_URL}${path}`;
    const headers: Record<string, string> = {};
    // Only set JSON content type when there's a body to avoid Fastify rejecting empty-body DELETE requests
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

// ── URI parsing ───────────────────────────────────────────────────────────────

export interface ParsedUri {
    scheme: string;
    mountId: string;
    path: string;
}

export function parseUri(uri: string): ParsedUri {
    const match = uri.match(/^([a-z]+):\/\/([^/]+)\/?(.*)$/);
    if (!match) {
        throw new Error(`Invalid URI: ${uri}. Expected format: <scheme>://<mountId>/<path>`);
    }
    return {
        scheme: match[1] ?? "",
        mountId: match[2] ?? "",
        path: "/" + (match[3] ?? ""),
    };
}

// ── Provider API ──────────────────────────────────────────────────────────────

export interface ProviderInfo {
    mountId: string;
    scheme: string;
    config: Record<string, string>;
}

export async function getProviders(): Promise<ProviderInfo[]> {
    return request<ProviderInfo[]>("/api/v1/providers");
}

export async function mountProvider(mountId: string, scheme: string, config: Record<string, string>): Promise<ProviderInfo> {
    return request<ProviderInfo>("/api/v1/providers", {
        method: "POST",
        body: JSON.stringify({ mountId, scheme, config }),
    });
}

export async function unmountProvider(mountId: string): Promise<void> {
    return request<void>(`/api/v1/providers/${mountId}`, { method: "DELETE" });
}

// ── File API ──────────────────────────────────────────────────────────────────

export async function listFiles(mountId: string, filePath: string): Promise<FileEntry[]> {
    const encodedPath = filePath.replace(/^\//, "");
    return request<FileEntry[]>(`/api/v1/files/${mountId}/${encodedPath}`);
}

export async function statFile(mountId: string, filePath: string): Promise<FileStat> {
    const encodedPath = filePath.replace(/^\//, "");
    return request<FileStat>(`/api/v1/files/${mountId}/${encodedPath}?stat`);
}

export async function readFile(mountId: string, filePath: string): Promise<ReadableStream<Uint8Array>> {
    const encodedPath = filePath.replace(/^\//, "");
    const url = `${BASE_URL}/api/v1/files/${mountId}/${encodedPath}`;
    const res = await fetch(url);
    if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
    }
    if (!res.body) {
        throw new Error("No response body");
    }
    return res.body;
}

export async function writeFile(mountId: string, filePath: string, data: Buffer | Uint8Array | ReadableStream<Uint8Array>): Promise<void> {
    const encodedPath = filePath.replace(/^\//, "");
    const url = `${BASE_URL}/api/v1/files/${mountId}/${encodedPath}`;
    const res = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/octet-stream" },
        body: data as BodyInit,
        // @ts-expect-error — Bun supports duplex for streaming but it's not in BunFetchRequestInit
        duplex: "half",
    });
    if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
    }
}

export async function deleteFile(mountId: string, filePath: string): Promise<void> {
    const encodedPath = filePath.replace(/^\//, "");
    return request<void>(`/api/v1/files/${mountId}/${encodedPath}`, { method: "DELETE" });
}

export async function moveFile(src: string, dest: string): Promise<void> {
    return request<void>("/api/v1/files/move", {
        method: "POST",
        body: JSON.stringify({ src, dest }),
    });
}

export async function copyFile(src: string, dest: string): Promise<void> {
    return request<void>("/api/v1/files/copy", {
        method: "POST",
        body: JSON.stringify({ src, dest }),
    });
}

export async function mkdirFile(mountId: string, filePath: string): Promise<{ path: string }> {
    const encodedPath = filePath.replace(/^\//, "");
    return request<{ path: string }>(`/api/v1/files/${mountId}/${encodedPath}`, { method: "PUT" });
}
