import * as fs from "node:fs/promises";
import * as path from "node:path";
import type { FileEntry, FileStat, ChangeEvent, StorageProvider } from "@file-manager/shared";

export class LocalProvider implements StorageProvider {
    private rootDir: string;

    constructor(rootDir: string) {
        this.rootDir = path.resolve(rootDir);
    }

    private resolve(filePath: string): string {
        const resolved = path.resolve(this.rootDir, filePath.replace(/^\//, ""));
        if (!resolved.startsWith(this.rootDir + path.sep) && resolved !== this.rootDir) {
            throw new Error(`Path traversal detected: ${filePath}`);
        }
        return resolved;
    }

    async list(filePath: string): Promise<FileEntry[]> {
        const absPath = this.resolve(filePath);
        const entries = await fs.readdir(absPath, { withFileTypes: true });

        const dirs: FileEntry[] = [];
        const files: FileEntry[] = [];

        for (const entry of entries) {
            const entryPath = path.join(filePath, entry.name);
            const stat = await fs.stat(path.join(absPath, entry.name));
            const fileEntry: FileEntry = {
                name: entry.name,
                path: entryPath,
                type: entry.isDirectory() ? "directory" : entry.isSymbolicLink() ? "symlink" : "file",
                size: stat.size,
                modifiedAt: stat.mtime,
            };

            if (entry.isDirectory()) {
                dirs.push(fileEntry);
            } else {
                files.push(fileEntry);
            }
        }

        dirs.sort((a, b) => a.name.localeCompare(b.name));
        files.sort((a, b) => a.name.localeCompare(b.name));

        return [...dirs, ...files];
    }

    async stat(filePath: string): Promise<FileStat> {
        const absPath = this.resolve(filePath);
        const s = await fs.stat(absPath);
        const name = path.basename(absPath);

        return {
            name,
            path: filePath,
            type: s.isDirectory() ? "directory" : s.isSymbolicLink() ? "symlink" : "file",
            size: s.size,
            createdAt: s.birthtime,
            modifiedAt: s.mtime,
        };
    }

    async *read(filePath: string): AsyncIterable<Buffer> {
        const absPath = this.resolve(filePath);
        const file = Bun.file(absPath);
        const stream = file.stream();
        const reader = stream.getReader();

        try {
            while (true) {
                const { done, value } = await reader.read();
                if (done) break;
                yield Buffer.from(value);
            }
        } finally {
            reader.releaseLock();
        }
    }

    async write(filePath: string, data: AsyncIterable<Buffer>): Promise<void> {
        const absPath = this.resolve(filePath);

        // Ensure parent directory exists
        await fs.mkdir(path.dirname(absPath), { recursive: true });

        const chunks: Buffer[] = [];
        for await (const chunk of data) {
            chunks.push(chunk);
        }
        await Bun.write(absPath, Buffer.concat(chunks));
    }

    async delete(filePath: string): Promise<void> {
        const absPath = this.resolve(filePath);
        await fs.rm(absPath, { recursive: true, force: true });
    }

    async move(src: string, dest: string): Promise<void> {
        const absSrc = this.resolve(src);
        const absDest = this.resolve(dest);
        await fs.mkdir(path.dirname(absDest), { recursive: true });
        await fs.rename(absSrc, absDest);
    }

    // eslint-disable-next-line @typescript-eslint/require-await
    async *watch(_filePath: string): AsyncIterable<ChangeEvent> {
        throw new Error("watch() not implemented in POC");
    }
}
