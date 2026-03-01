// ── Schemas: generated types, validators, and Fastify schema objects ──────────

import type { FileEntry } from "@file-manager/schemas";
export type { FileEntry };

export type { MountInfo, MoveRequest, LocalProviderConfig, Operation } from "@file-manager/schemas";
export {
    mountInfoSchema,
    assertLocalProviderConfig,
    moveRequestSchema,
    moveRequestSchema as srcDestRequestSchema,
    // fileStatSchema is the wire-format counterpart to FileStat below (timestamps as strings)
    fileStatSchema,
    operationSchema,
} from "@file-manager/schemas";

// ── File entry types ──────────────────────────────────────────────────────────

/**
 * Internal/provider representation of file metadata. Uses `Date` objects and is
 * never serialized directly. For the wire format (ISO 8601 strings) used in
 * Fastify response schemas, see `fileStatSchema` from `@file-manager/schemas`.
 */
export interface FileStat {
    name: string;
    path: string;
    type: "file" | "directory" | "symlink";
    size: number;
    createdAt: Date;
    modifiedAt: Date;
    mimeType?: string;
}

// ── Change events ─────────────────────────────────────────────────────────────

export interface ChangeEvent {
    type: "create" | "modify" | "delete" | "rename";
    path: string;
    oldPath?: string; // for rename
}

// ── Provider types ────────────────────────────────────────────────────────────

export type ProviderScheme = "local" | "s3" | "gcs" | "sftp" | "smb";

export interface ProviderMount {
    mountId: string;
    scheme: ProviderScheme;
    config: Record<string, string>;
    provider: StorageProvider;
}

export interface StorageProvider {
    list(path: string): Promise<FileEntry[]>;
    stat(path: string): Promise<FileStat>;
    read(path: string): AsyncIterable<Buffer>;
    write(path: string, data: AsyncIterable<Buffer>): Promise<void>;
    delete(path: string): Promise<void>;
    move(src: string, dest: string): Promise<void>;
    mkdir(path: string): Promise<void>;
}
