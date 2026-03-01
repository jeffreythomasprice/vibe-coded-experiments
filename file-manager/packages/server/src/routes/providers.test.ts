import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { createServer } from "../server.js";

describe("providers routes", () => {
    let server: Awaited<ReturnType<typeof createServer>>;
    let baseUrl: string;
    let tmpDir: string;

    beforeEach(async () => {
        tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "providers-test-"));
        server = await createServer();
        const address = await server.listen({ port: 0, host: "127.0.0.1" });
        baseUrl = address;
    });

    afterEach(async () => {
        await server.close();
        await fs.rm(tmpDir, { recursive: true, force: true });
    });

    test("GET /api/v1/providers returns empty array on fresh server", async () => {
        const res = await fetch(`${baseUrl}/api/v1/providers`);
        expect(res.status).toBe(200);
        const body = await res.json();
        expect(body).toEqual([]);
    });

    test("POST /api/v1/providers with valid local config returns 201 with mountInfo", async () => {
        const res = await fetch(`${baseUrl}/api/v1/providers`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ mountId: "docs", scheme: "local", config: { rootDir: tmpDir } }),
        });
        expect(res.status).toBe(201);
        const body = await res.json() as { mountId: string; scheme: string; config: Record<string, string> };
        expect(body.mountId).toBe("docs");
        expect(body.scheme).toBe("local");
        expect(body.config.rootDir).toBe(tmpDir);
    });

    test("POST /api/v1/providers with unknown scheme returns 400", async () => {
        const res = await fetch(`${baseUrl}/api/v1/providers`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ mountId: "ftp-mount", scheme: "sftp", config: { host: "example.com" } }),
        });
        expect(res.status).toBe(400);
    });

    test("POST /api/v1/providers with missing rootDir for local scheme returns 400", async () => {
        const res = await fetch(`${baseUrl}/api/v1/providers`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ mountId: "docs", scheme: "local", config: {} }),
        });
        expect(res.status).toBe(400);
    });

    test("POST /api/v1/providers duplicate mountId returns 409", async () => {
        const opts = {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ mountId: "docs", scheme: "local", config: { rootDir: tmpDir } }),
        };

        const first = await fetch(`${baseUrl}/api/v1/providers`, opts);
        expect(first.status).toBe(201);

        const second = await fetch(`${baseUrl}/api/v1/providers`, opts);
        expect(second.status).toBe(409);
    });

    test("DELETE /api/v1/providers/:mountId returns 204", async () => {
        // Mount first
        await fetch(`${baseUrl}/api/v1/providers`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ mountId: "docs", scheme: "local", config: { rootDir: tmpDir } }),
        });

        const res = await fetch(`${baseUrl}/api/v1/providers/docs`, { method: "DELETE" });
        expect(res.status).toBe(204);

        // Confirm it's gone
        const list = await fetch(`${baseUrl}/api/v1/providers`);
        expect(await list.json()).toEqual([]);
    });

    test("DELETE /api/v1/providers/unknown returns 404", async () => {
        const res = await fetch(`${baseUrl}/api/v1/providers/unknown`, { method: "DELETE" });
        expect(res.status).toBe(404);
    });
});
