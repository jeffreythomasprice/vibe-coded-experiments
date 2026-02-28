// ── File entry types ──────────────────────────────────────────────────────────

export type FileEntryType = "file" | "directory" | "symlink";

export interface FileStat {
    name: string;
    path: string;
    type: FileEntryType;
    size: number;
    createdAt: Date;
    modifiedAt: Date;
    mimeType?: string;
}

export interface FileEntry {
    name: string;
    path: string;
    type: FileEntryType;
    size: number;
    modifiedAt: Date;
}

// ── Change events ─────────────────────────────────────────────────────────────

export type ChangeEventType = "create" | "modify" | "delete" | "rename";

export interface ChangeEvent {
    type: ChangeEventType;
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
    watch(path: string): AsyncIterable<ChangeEvent>;
}

// ── Sync types (unused in POC, included for completeness) ────────────────────

export type SyncDirection = "push" | "pull" | "bidirectional";

export interface SyncRule {
    id: string;
    srcUri: string;
    destUri: string;
    direction: SyncDirection;
    schedule?: string; // cron expression
}
