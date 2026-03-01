import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { createServer } from "../server.js";
import { LocalProvider } from "../providers/local.js";

async function waitForOperation(baseUrl: string, operationId: string, timeoutMs: number = 5000): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        const res = await fetch(`${baseUrl}/api/v1/operations/${operationId}`);
        if (res.ok) {
            const op = await res.json() as { status: string };
            if (op.status === "completed") return;
            if (op.status === "failed") throw new Error("Operation failed");
        }
        await new Promise((r) => setTimeout(r, 10));
    }
    throw new Error(`Operation ${operationId} did not complete within ${timeoutMs}ms`);
}

describe("file routes", () => {
    let server: Awaited<ReturnType<typeof createServer>>;
    let baseUrl: string;
    let tmpDir: string;
    const mountId = "test";

    beforeEach(async () => {
        tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "files-test-"));
        server = await createServer();
        // Manually mount a LocalProvider — avoids going through the HTTP layer
        await server.registry.mount(mountId, "local", { rootDir: tmpDir }, new LocalProvider(tmpDir));
        const address = await server.listen({ port: 0, host: "127.0.0.1" });
        baseUrl = address;
    });

    afterEach(async () => {
        await server.close();
        await fs.rm(tmpDir, { recursive: true, force: true });
    });

    // ── GET (list) ────────────────────────────────────────────────────────────

    test("GET /api/v1/files/:mountId/ on directory returns 200 with FileEntry[]", async () => {
        await Bun.write(path.join(tmpDir, "file.txt"), "hello");

        const res = await fetch(`${baseUrl}/api/v1/files/${mountId}/`);
        expect(res.status).toBe(200);
        const entries = await res.json() as Array<{ name: string; type: string }>;
        expect(entries).toHaveLength(1);
        expect(entries[0]!.name).toBe("file.txt");
        expect(entries[0]!.type).toBe("file");
    });

    test("GET /api/v1/files/:mountId/file.txt for a file returns 200 with file bytes", async () => {
        const content = "hello file";
        await Bun.write(path.join(tmpDir, "file.txt"), content);

        const res = await fetch(`${baseUrl}/api/v1/files/${mountId}/file.txt`);
        expect(res.status).toBe(200);
        expect(await res.text()).toBe(content);
    });

    test("GET /api/v1/files/:mountId/file.txt?stat returns 200 with stat JSON", async () => {
        await Bun.write(path.join(tmpDir, "file.txt"), "stat me");

        const res = await fetch(`${baseUrl}/api/v1/files/${mountId}/file.txt?stat`);
        expect(res.status).toBe(200);
        const stat = await res.json() as { name: string; type: string; size: number };
        expect(stat.name).toBe("file.txt");
        expect(stat.type).toBe("file");
        expect(stat.size).toBe(7); // "stat me" is 7 bytes
    });

    test("GET /api/v1/files/unknown/ returns 404", async () => {
        const res = await fetch(`${baseUrl}/api/v1/files/unknown/`);
        expect(res.status).toBe(404);
    });

    // ── POST (write) ──────────────────────────────────────────────────────────

    test("POST /api/v1/files/:mountId/new.txt with octet-stream returns 201 with path", async () => {
        const res = await fetch(`${baseUrl}/api/v1/files/${mountId}/new.txt`, {
            method: "POST",
            headers: { "Content-Type": "application/octet-stream" },
            body: Buffer.from("new content"),
        });
        expect(res.status).toBe(201);
        const body = await res.json() as { path: string };
        expect(body.path).toBe("/new.txt");

        // Verify the file was actually written
        const data = await fs.readFile(path.join(tmpDir, "new.txt"));
        expect(data.toString()).toBe("new content");
    });

    test("POST /api/v1/files/:mountId/large.bin accepts uploads larger than 1MB", async () => {
        const size = 2 * 1024 * 1024; // 2 MB
        const buf = Buffer.alloc(size, 0x42); // fill with 'B'

        const res = await fetch(`${baseUrl}/api/v1/files/${mountId}/large.bin`, {
            method: "POST",
            headers: { "Content-Type": "application/octet-stream" },
            body: buf,
        });
        expect(res.status).toBe(201);

        const written = await fs.readFile(path.join(tmpDir, "large.bin"));
        expect(written.length).toBe(size);
    });

    test("POST /api/v1/files/:mountId/streamed.bin streams upload without full buffering", async () => {
        // Build a ReadableStream that yields multiple chunks
        const chunkSize = 64 * 1024; // 64 KB
        const chunkCount = 16; // 1 MB total
        const expectedSize = chunkSize * chunkCount;

        const stream = new ReadableStream({
            start(controller) {
                for (let i = 0; i < chunkCount; i++) {
                    controller.enqueue(new Uint8Array(chunkSize).fill(i & 0xff));
                }
                controller.close();
            },
        });

        const res = await fetch(`${baseUrl}/api/v1/files/${mountId}/streamed.bin`, {
            method: "POST",
            headers: { "Content-Type": "application/octet-stream" },
            body: stream,
            // @ts-expect-error -- Bun supports duplex: "half" for streaming uploads
            duplex: "half",
        });
        expect(res.status).toBe(201);

        const written = await fs.readFile(path.join(tmpDir, "streamed.bin"));
        expect(written.length).toBe(expectedSize);

        // Verify first and last chunk content
        expect(written[0]).toBe(0x00);
        expect(written[chunkSize]).toBe(0x01);
        expect(written[chunkSize * (chunkCount - 1)]).toBe((chunkCount - 1) & 0xff);
    });

    // ── PUT (mkdir) ───────────────────────────────────────────────────────────

    test("PUT /api/v1/files/:mountId/newdir returns 201 with path", async () => {
        const res = await fetch(`${baseUrl}/api/v1/files/${mountId}/newdir`, { method: "PUT" });
        expect(res.status).toBe(201);
        const body = await res.json() as { path: string };
        expect(body.path).toBe("/newdir");

        // Verify directory is visible in subsequent listing
        const listRes = await fetch(`${baseUrl}/api/v1/files/${mountId}/`);
        const entries = await listRes.json() as Array<{ name: string; type: string }>;
        expect(entries.some((e) => e.name === "newdir" && e.type === "directory")).toBe(true);
    });

    test("PUT /api/v1/files/unknown/newdir returns 404", async () => {
        const res = await fetch(`${baseUrl}/api/v1/files/unknown/newdir`, { method: "PUT" });
        expect(res.status).toBe(404);
    });

    // ── DELETE ────────────────────────────────────────────────────────────────

    test("DELETE /api/v1/files/:mountId/file.txt returns 200 with operationId", async () => {
        await Bun.write(path.join(tmpDir, "file.txt"), "delete me");

        const res = await fetch(`${baseUrl}/api/v1/files/${mountId}/file.txt`, { method: "DELETE" });
        expect(res.status).toBe(200);
        const body = await res.json() as { operationId: string };
        expect(typeof body.operationId).toBe("string");

        await expect(fs.stat(path.join(tmpDir, "file.txt"))).rejects.toThrow();
    });

    // ── Upload tracking ──────────────────────────────────────────────────────

    test("POST /api/v1/files/:mountId/tracked.txt creates a tracked upload operation", async () => {
        const res = await fetch(`${baseUrl}/api/v1/files/${mountId}/tracked.txt`, {
            method: "POST",
            headers: { "Content-Type": "application/octet-stream" },
            body: Buffer.from("tracked upload"),
        });
        expect(res.status).toBe(201);
        const body = await res.json() as { path: string; operationId: string };
        expect(body.path).toBe("/tracked.txt");
        expect(typeof body.operationId).toBe("string");

        // Verify operation is completed
        const opRes = await fetch(`${baseUrl}/api/v1/operations/${body.operationId}`);
        expect(opRes.ok).toBe(true);
        const op = await opRes.json() as { type: string; status: string; src: string; dest?: string };
        expect(op.type).toBe("upload");
        expect(op.status).toBe("completed");
        expect(op.src).toBe(`${mountId}://tracked.txt`);
        expect(op.dest).toBeUndefined();
    });

    // ── Delete tracking ─────────────────────────────────────────────────────

    test("DELETE /api/v1/files/:mountId/file.txt creates a tracked delete operation", async () => {
        await Bun.write(path.join(tmpDir, "del.txt"), "delete me");

        const res = await fetch(`${baseUrl}/api/v1/files/${mountId}/del.txt`, { method: "DELETE" });
        expect(res.status).toBe(200);
        const body = await res.json() as { operationId: string };
        expect(typeof body.operationId).toBe("string");

        // Verify operation is completed
        const opRes = await fetch(`${baseUrl}/api/v1/operations/${body.operationId}`);
        expect(opRes.ok).toBe(true);
        const op = await opRes.json() as { type: string; status: string; src: string; dest?: string };
        expect(op.type).toBe("delete");
        expect(op.status).toBe("completed");
        expect(op.src).toBe(`${mountId}://del.txt`);
        expect(op.dest).toBeUndefined();
    });

    // ── POST /files/move ──────────────────────────────────────────────────────

    test("POST /api/v1/files/move same-mount returns 200 with src and dest", async () => {
        await Bun.write(path.join(tmpDir, "source.txt"), "move content");

        const res = await fetch(`${baseUrl}/api/v1/files/move`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                src: `${mountId}://source.txt`,
                dest: `${mountId}://dest.txt`,
            }),
        });
        expect(res.status).toBe(200);
        const body = await res.json() as { src: string; dest: string; operationId?: string };
        expect(body.src).toBe(`${mountId}://source.txt`);
        expect(body.dest).toBe(`${mountId}://dest.txt`);
        expect(typeof body.operationId).toBe("string");

        // Verify the move happened
        await expect(fs.stat(path.join(tmpDir, "source.txt"))).rejects.toThrow();
        const data = await fs.readFile(path.join(tmpDir, "dest.txt"));
        expect(data.toString()).toBe("move content");
    });

    test("POST /api/v1/files/move with invalid URI returns 400", async () => {
        const res = await fetch(`${baseUrl}/api/v1/files/move`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ src: "not-a-uri", dest: "also-not-a-uri" }),
        });
        expect(res.status).toBe(400);
    });

    // ── POST /files/copy ──────────────────────────────────────────────────────

    test("POST /api/v1/files/copy same-mount returns 202 with src, dest, and operationId, source preserved", async () => {
        await Bun.write(path.join(tmpDir, "original.txt"), "copy content");

        const res = await fetch(`${baseUrl}/api/v1/files/copy`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                src: `${mountId}://original.txt`,
                dest: `${mountId}://copy.txt`,
            }),
        });
        expect(res.status).toBe(202);
        const body = await res.json() as { src: string; dest: string; operationId: string };
        expect(body.src).toBe(`${mountId}://original.txt`);
        expect(body.dest).toBe(`${mountId}://copy.txt`);
        expect(typeof body.operationId).toBe("string");

        // Wait for background operation to complete
        await waitForOperation(baseUrl, body.operationId);

        // Source still exists (copy, not move)
        const srcData = await fs.readFile(path.join(tmpDir, "original.txt"));
        expect(srcData.toString()).toBe("copy content");

        // Destination has same content
        const destData = await fs.readFile(path.join(tmpDir, "copy.txt"));
        expect(destData.toString()).toBe("copy content");
    });

    test("POST /api/v1/files/copy with invalid URI returns 400", async () => {
        const res = await fetch(`${baseUrl}/api/v1/files/copy`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ src: "not-a-uri", dest: `${mountId}://dest.txt` }),
        });
        expect(res.status).toBe(400);
    });

    test("POST /api/v1/files/copy with unknown mount returns 404", async () => {
        const res = await fetch(`${baseUrl}/api/v1/files/copy`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                src: `no-such-mount://file.txt`,
                dest: `${mountId}://dest.txt`,
            }),
        });
        expect(res.status).toBe(404);
    });

    test("POST /api/v1/files/copy copies a directory recursively (same-mount)", async () => {
        // Create src/a.txt and src/sub/b.txt
        await fs.mkdir(path.join(tmpDir, "src", "sub"), { recursive: true });
        await Bun.write(path.join(tmpDir, "src", "a.txt"), "file a");
        await Bun.write(path.join(tmpDir, "src", "sub", "b.txt"), "file b");

        const res = await fetch(`${baseUrl}/api/v1/files/copy`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                src: `${mountId}://src`,
                dest: `${mountId}://dest`,
            }),
        });
        expect(res.status).toBe(202);
        const body = await res.json() as { operationId: string };

        // Wait for the background operation to complete
        await waitForOperation(baseUrl, body.operationId);

        // Source still exists
        expect((await fs.stat(path.join(tmpDir, "src", "a.txt"))).isFile()).toBe(true);

        // Destination has both files
        const destA = await fs.readFile(path.join(tmpDir, "dest", "a.txt"));
        expect(destA.toString()).toBe("file a");
        const destB = await fs.readFile(path.join(tmpDir, "dest", "sub", "b.txt"));
        expect(destB.toString()).toBe("file b");
    });

    test("POST /api/v1/files/move moves a directory recursively cross-mount", async () => {
        // Set up a second tmpDir + mount
        const tmpDir2 = await fs.mkdtemp(path.join(os.tmpdir(), "files-test2-"));
        const mountId2 = "test2";
        await server.registry.mount(mountId2, "local", { rootDir: tmpDir2 }, new LocalProvider(tmpDir2));

        try {
            // Create dir/file.txt in first mount
            await fs.mkdir(path.join(tmpDir, "dir"), { recursive: true });
            await Bun.write(path.join(tmpDir, "dir", "file.txt"), "cross-mount content");

            const res = await fetch(`${baseUrl}/api/v1/files/move`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    src: `${mountId}://dir`,
                    dest: `${mountId2}://dir`,
                }),
            });
            expect(res.status).toBe(200);
            const body = await res.json() as { src: string; dest: string; operationId?: string };
            expect(typeof body.operationId).toBe("string");

            // Wait for the background operation to complete
            await waitForOperation(baseUrl, body.operationId!);

            // Source is gone
            await expect(fs.stat(path.join(tmpDir, "dir"))).rejects.toThrow();

            // Destination has the file
            const destFile = await fs.readFile(path.join(tmpDir2, "dir", "file.txt"));
            expect(destFile.toString()).toBe("cross-mount content");
        } finally {
            await fs.rm(tmpDir2, { recursive: true, force: true });
        }
    });
});
