import { mkdirSync, existsSync } from "node:fs";

interface CacheMeta {
    url: string;
    status: number;
    headers: Record<string, string>;
    fetchedAt: number;
}

export interface CachedResponse {
    status: number;
    headers: Record<string, string>;
    body: Uint8Array;
}

function hashUrl(url: string): string {
    const hasher = new Bun.CryptoHasher("sha256");
    hasher.update(url);
    return hasher.digest("hex").slice(0, 16);
}

export class FileCache {
    private dir: string;
    private defaultTtlMs: number;

    constructor(dir: string, defaultTtlMs: number) {
        this.dir = dir;
        this.defaultTtlMs = defaultTtlMs;
        if (!existsSync(this.dir)) {
            mkdirSync(this.dir, { recursive: true });
        }
    }

    async get(url: string, ttlMs?: number): Promise<CachedResponse | null> {
        const hash = hashUrl(url);
        const metaFile = Bun.file(`${this.dir}/${hash}.meta.json`);

        if (!(await metaFile.exists())) return null;

        const meta: CacheMeta = await metaFile.json();
        const ttl = ttlMs ?? this.defaultTtlMs;

        if (Date.now() - meta.fetchedAt > ttl) return null;

        const bodyFile = Bun.file(`${this.dir}/${hash}.body`);
        if (!(await bodyFile.exists())) return null;

        return {
            status: meta.status,
            headers: meta.headers,
            body: new Uint8Array(await bodyFile.arrayBuffer()),
        };
    }

    async put(
        url: string,
        status: number,
        headers: Record<string, string>,
        body: Uint8Array,
    ): Promise<void> {
        const hash = hashUrl(url);
        const meta: CacheMeta = {
            url,
            status,
            headers,
            fetchedAt: Date.now(),
        };

        await Promise.all([
            Bun.write(`${this.dir}/${hash}.meta.json`, JSON.stringify(meta)),
            Bun.write(`${this.dir}/${hash}.body`, body),
        ]);
    }
}
