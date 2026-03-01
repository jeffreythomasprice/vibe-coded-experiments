import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { createServer } from "../server.js";
import { LocalProvider } from "../providers/local.js";
import type { Operation } from "@file-manager/shared";

async function waitForOperation(baseUrl: string, operationId: string, timeoutMs: number = 5000): Promise<Operation> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        const res = await fetch(`${baseUrl}/api/v1/operations/${operationId}`);
        if (res.ok) {
            const op = await res.json() as Operation;
            if (op.status === "completed") return op;
            if (op.status === "failed") throw new Error(`Operation failed: ${op.error}`);
        }
        await new Promise((r) => setTimeout(r, 10));
    }
    throw new Error(`Operation ${operationId} did not complete within ${timeoutMs}ms`);
}

async function triggerCopy(baseUrl: string, src: string, dest: string): Promise<{ operationId: string }> {
    const res = await fetch(`${baseUrl}/api/v1/files/copy`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ src, dest }),
    });
    expect(res.status).toBe(202);
    return res.json() as Promise<{ operationId: string }>;
}

describe("operation routes", () => {
    let server: Awaited<ReturnType<typeof createServer>>;
    let baseUrl: string;
    let tmpDir: string;
    const mountId = "test";

    beforeEach(async () => {
        tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "ops-test-"));
        server = await createServer();
        await server.registry.mount(mountId, "local", { rootDir: tmpDir }, new LocalProvider(tmpDir));
        const address = await server.listen({ port: 0, host: "127.0.0.1" });
        baseUrl = address;
    });

    afterEach(async () => {
        await server.close();
        await fs.rm(tmpDir, { recursive: true, force: true });
    });

    // ── GET /operations ───────────────────────────────────────────────────────

    test("GET /api/v1/operations returns empty array initially", async () => {
        const res = await fetch(`${baseUrl}/api/v1/operations`);
        expect(res.status).toBe(200);
        const ops = await res.json() as Operation[];
        expect(ops).toHaveLength(0);
    });

    test("copy creates a tracked operation visible via GET /operations", async () => {
        await Bun.write(path.join(tmpDir, "file.txt"), "hello");

        const { operationId } = await triggerCopy(baseUrl, `${mountId}://file.txt`, `${mountId}://copy.txt`);
        await waitForOperation(baseUrl, operationId);

        const res = await fetch(`${baseUrl}/api/v1/operations`);
        const ops = await res.json() as Operation[];
        expect(ops).toHaveLength(1);
        expect(ops[0]!.type).toBe("copy");
        expect(ops[0]!.status).toBe("completed");
        expect(ops[0]!.totalFiles).toBe(1);
        expect(ops[0]!.processedFiles).toBe(1);
    });

    // ── GET /operations/:id ───────────────────────────────────────────────────

    test("GET /api/v1/operations/:id returns 404 for unknown id", async () => {
        const res = await fetch(`${baseUrl}/api/v1/operations/nonexistent`);
        expect(res.status).toBe(404);
    });

    test("GET /api/v1/operations/:id returns 200 for valid operation", async () => {
        await Bun.write(path.join(tmpDir, "file.txt"), "data");

        const { operationId } = await triggerCopy(baseUrl, `${mountId}://file.txt`, `${mountId}://copy.txt`);
        await waitForOperation(baseUrl, operationId);

        const res = await fetch(`${baseUrl}/api/v1/operations/${operationId}`);
        expect(res.status).toBe(200);
        const op = await res.json() as Operation;
        expect(op.id).toBe(operationId);
        expect(op.type).toBe("copy");
    });

    // ── Status filter ─────────────────────────────────────────────────────────

    test("GET /api/v1/operations?status=completed filters correctly", async () => {
        await Bun.write(path.join(tmpDir, "a.txt"), "aaa");

        const { operationId } = await triggerCopy(baseUrl, `${mountId}://a.txt`, `${mountId}://b.txt`);
        await waitForOperation(baseUrl, operationId);

        const completed = await fetch(`${baseUrl}/api/v1/operations?status=completed`);
        const completedOps = await completed.json() as Operation[];
        expect(completedOps).toHaveLength(1);

        const pending = await fetch(`${baseUrl}/api/v1/operations?status=pending`);
        const pendingOps = await pending.json() as Operation[];
        expect(pendingOps).toHaveLength(0);
    });

    test("GET /api/v1/operations?status=invalid returns 400", async () => {
        const res = await fetch(`${baseUrl}/api/v1/operations?status=invalid`);
        expect(res.status).toBe(400);
    });

    // ── Recursive copy tracking ───────────────────────────────────────────────

    test("recursive copy tracks correct file count and bytes", async () => {
        await fs.mkdir(path.join(tmpDir, "dir", "sub"), { recursive: true });
        await Bun.write(path.join(tmpDir, "dir", "a.txt"), "aaa");    // 3 bytes
        await Bun.write(path.join(tmpDir, "dir", "sub", "b.txt"), "bbbbb"); // 5 bytes

        const { operationId } = await triggerCopy(baseUrl, `${mountId}://dir`, `${mountId}://dir-copy`);
        await waitForOperation(baseUrl, operationId);

        const res = await fetch(`${baseUrl}/api/v1/operations`);
        const ops = await res.json() as Operation[];
        expect(ops).toHaveLength(1);
        expect(ops[0]!.totalFiles).toBe(2);
        expect(ops[0]!.processedFiles).toBe(2);
        expect(ops[0]!.totalBytes).toBe(8);
        expect(ops[0]!.processedBytes).toBe(8);
        expect(ops[0]!.status).toBe("completed");
    });

    // ── Cross-mount move tracking ─────────────────────────────────────────────

    test("cross-mount move creates a tracked move operation", async () => {
        const tmpDir2 = await fs.mkdtemp(path.join(os.tmpdir(), "ops-test2-"));
        const mountId2 = "test2";
        await server.registry.mount(mountId2, "local", { rootDir: tmpDir2 }, new LocalProvider(tmpDir2));

        try {
            await Bun.write(path.join(tmpDir, "move-me.txt"), "movable");

            const moveRes = await fetch(`${baseUrl}/api/v1/files/move`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    src: `${mountId}://move-me.txt`,
                    dest: `${mountId2}://moved.txt`,
                }),
            });
            expect(moveRes.status).toBe(200);
            const body = await moveRes.json() as { operationId: string };

            await waitForOperation(baseUrl, body.operationId);

            const res = await fetch(`${baseUrl}/api/v1/operations`);
            const ops = await res.json() as Operation[];
            expect(ops).toHaveLength(1);
            expect(ops[0]!.type).toBe("move");
            expect(ops[0]!.status).toBe("completed");
        } finally {
            await fs.rm(tmpDir2, { recursive: true, force: true });
        }
    });

    // ── SSE ───────────────────────────────────────────────────────────────────

    test("SSE stream delivers events during a copy", async () => {
        await Bun.write(path.join(tmpDir, "sse-test.txt"), "sse data");

        // Start SSE connection first
        const controller = new AbortController();
        const sseRes = await fetch(`${baseUrl}/api/v1/operations/events`, {
            signal: controller.signal,
        });
        expect(sseRes.status).toBe(200);
        expect(sseRes.headers.get("content-type")).toBe("text/event-stream");

        // Perform a copy while SSE is connected — copy runs in background so events flow through SSE
        await triggerCopy(baseUrl, `${mountId}://sse-test.txt`, `${mountId}://sse-copy.txt`);

        // Read the SSE stream
        const reader = sseRes.body!.getReader();
        const decoder = new TextDecoder();
        let accumulated = "";

        // Read chunks until we see a "completed" event or timeout
        const readWithTimeout = async () => {
            const timeout = setTimeout(() => controller.abort(), 2000);
            try {
                while (true) {
                    const { done, value } = await reader.read();
                    if (done) break;
                    accumulated += decoder.decode(value, { stream: true });
                    if (accumulated.includes("event: completed")) break;
                }
            } catch {
                // AbortError on timeout is expected
            } finally {
                clearTimeout(timeout);
            }
        };
        await readWithTimeout();
        controller.abort();

        // Parse SSE events
        const events = accumulated
            .split("\n\n")
            .filter((block) => block.includes("event:") && block.includes("data:"));

        expect(events.length).toBeGreaterThanOrEqual(1);

        // Should have a "created" event
        const createdEvent = events.find((e) => e.includes("event: created"));
        expect(createdEvent).toBeDefined();

        // Should have a "completed" event
        const completedEvent = events.find((e) => e.includes("event: completed"));
        expect(completedEvent).toBeDefined();

        // Parse the completed event data
        const dataLine = completedEvent!.split("\n").find((l) => l.startsWith("data:"));
        const op = JSON.parse(dataLine!.slice(5).trim()) as Operation;
        expect(op.status).toBe("completed");
        expect(op.type).toBe("copy");
    });

    // ── Same-mount move IS tracked ──────────────────────────────────────────

    test("same-mount move creates a tracked operation", async () => {
        await Bun.write(path.join(tmpDir, "src.txt"), "data");

        const moveRes = await fetch(`${baseUrl}/api/v1/files/move`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                src: `${mountId}://src.txt`,
                dest: `${mountId}://dst.txt`,
            }),
        });
        const moveBody = await moveRes.json() as { operationId?: string };
        expect(typeof moveBody.operationId).toBe("string");

        const res = await fetch(`${baseUrl}/api/v1/operations`);
        const ops = await res.json() as Operation[];
        expect(ops.length).toBeGreaterThanOrEqual(1);
        const moveOp = ops.find((op) => op.type === "move");
        expect(moveOp).toBeDefined();
        expect(moveOp!.status).toBe("completed");
    });
});
