import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { LocalProvider } from "./local.js";

async function collect(iter: AsyncIterable<Buffer>): Promise<Buffer> {
    const chunks: Buffer[] = [];
    for await (const chunk of iter) {
        chunks.push(chunk);
    }
    return Buffer.concat(chunks);
}

async function makeIterable(buf: Buffer): AsyncIterable<Buffer> {
    async function* gen() {
        yield buf;
    }
    return gen();
}

describe("LocalProvider", () => {
    let tmpDir: string;
    let provider: LocalProvider;

    beforeEach(async () => {
        tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "local-provider-test-"));
        provider = new LocalProvider(tmpDir);
    });

    afterEach(async () => {
        await fs.rm(tmpDir, { recursive: true, force: true });
    });

    // ── Path traversal ────────────────────────────────────────────────────────

    test("stat() throws on path traversal attempt", async () => {
        await expect(provider.stat("../outside")).rejects.toThrow("Path traversal detected");
    });

    test("stat() throws on encoded traversal attempt", async () => {
        await expect(provider.stat("../../etc/passwd")).rejects.toThrow("Path traversal detected");
    });

    // ── list() ────────────────────────────────────────────────────────────────

    test("list() returns dirs first then files, both sorted alphabetically", async () => {
        await fs.mkdir(path.join(tmpDir, "zebra"));
        await fs.mkdir(path.join(tmpDir, "alpha"));
        await Bun.write(path.join(tmpDir, "z-file.txt"), "z");
        await Bun.write(path.join(tmpDir, "a-file.txt"), "a");

        const entries = await provider.list("/");
        expect(entries).toHaveLength(4);
        expect(entries[0]!.name).toBe("alpha");
        expect(entries[0]!.type).toBe("directory");
        expect(entries[1]!.name).toBe("zebra");
        expect(entries[1]!.type).toBe("directory");
        expect(entries[2]!.name).toBe("a-file.txt");
        expect(entries[2]!.type).toBe("file");
        expect(entries[3]!.name).toBe("z-file.txt");
        expect(entries[3]!.type).toBe("file");
    });

    test("list() returns empty array for empty directory", async () => {
        const entries = await provider.list("/");
        expect(entries).toHaveLength(0);
    });

    // ── stat() ────────────────────────────────────────────────────────────────

    test("stat() returns correct info for a file", async () => {
        const content = "hello world";
        await Bun.write(path.join(tmpDir, "test.txt"), content);

        const stat = await provider.stat("/test.txt");
        expect(stat.name).toBe("test.txt");
        expect(stat.type).toBe("file");
        expect(stat.size).toBe(Buffer.byteLength(content));
        expect(stat.path).toBe("/test.txt");
    });

    test("stat() returns correct info for a directory", async () => {
        await fs.mkdir(path.join(tmpDir, "subdir"));

        const stat = await provider.stat("/subdir");
        expect(stat.name).toBe("subdir");
        expect(stat.type).toBe("directory");
        expect(stat.path).toBe("/subdir");
    });

    // ── read() ────────────────────────────────────────────────────────────────

    test("read() returns exact content written to file", async () => {
        const content = "exact content test";
        await Bun.write(path.join(tmpDir, "read-test.txt"), content);

        const data = await collect(provider.read("/read-test.txt"));
        expect(data.toString()).toBe(content);
    });

    test("read() handles binary content", async () => {
        const binary = Buffer.from([0x00, 0x01, 0x02, 0xff, 0xfe]);
        await Bun.write(path.join(tmpDir, "binary.bin"), binary);

        const data = await collect(provider.read("/binary.bin"));
        expect(data).toEqual(binary);
    });

    // ── write() ───────────────────────────────────────────────────────────────

    test("write() creates a file with the given content", async () => {
        const content = Buffer.from("written content");
        await provider.write("/new-file.txt", await makeIterable(content));

        const data = await fs.readFile(path.join(tmpDir, "new-file.txt"));
        expect(data.toString()).toBe("written content");
    });

    test("write() creates parent directories automatically", async () => {
        const content = Buffer.from("nested");
        await provider.write("/a/b/c/file.txt", await makeIterable(content));

        const exists = await fs.stat(path.join(tmpDir, "a", "b", "c", "file.txt"));
        expect(exists.isFile()).toBe(true);
    });

    // ── delete() ─────────────────────────────────────────────────────────────

    test("delete() removes a file", async () => {
        await Bun.write(path.join(tmpDir, "to-delete.txt"), "bye");

        await provider.delete("/to-delete.txt");

        await expect(fs.stat(path.join(tmpDir, "to-delete.txt"))).rejects.toThrow();
    });

    test("delete() removes a directory recursively", async () => {
        await fs.mkdir(path.join(tmpDir, "dir-to-delete", "nested"), { recursive: true });
        await Bun.write(path.join(tmpDir, "dir-to-delete", "nested", "file.txt"), "hi");

        await provider.delete("/dir-to-delete");

        await expect(fs.stat(path.join(tmpDir, "dir-to-delete"))).rejects.toThrow();
    });

    // ── move() ────────────────────────────────────────────────────────────────

    test("move() renames a file within the same directory", async () => {
        await Bun.write(path.join(tmpDir, "original.txt"), "move me");

        await provider.move("/original.txt", "/renamed.txt");

        await expect(fs.stat(path.join(tmpDir, "original.txt"))).rejects.toThrow();
        const data = await fs.readFile(path.join(tmpDir, "renamed.txt"));
        expect(data.toString()).toBe("move me");
    });

    test("move() creates destination parent directories if needed", async () => {
        await Bun.write(path.join(tmpDir, "file.txt"), "content");

        await provider.move("/file.txt", "/new/nested/dir/file.txt");

        await expect(fs.stat(path.join(tmpDir, "file.txt"))).rejects.toThrow();
        const data = await fs.readFile(path.join(tmpDir, "new", "nested", "dir", "file.txt"));
        expect(data.toString()).toBe("content");
    });
});
