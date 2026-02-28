import { describe, test, expect, beforeAll, afterAll } from "bun:test";
import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { createServer } from "./server.js";

const INTEGRATION = process.env["INTEGRATION"] === "1";
const CLI_PATH = path.resolve(import.meta.dir, "../../cli/src/index.ts");

interface RunResult {
    exitCode: number;
    stdout: string;
    stderr: string;
}

async function run(args: string[], serverUrl: string): Promise<RunResult> {
    const proc = Bun.spawn(["bun", "run", CLI_PATH, ...args], {
        env: { ...process.env, FILE_MANAGER_API_URL: serverUrl },
        stdout: "pipe",
        stderr: "pipe",
    });
    const [exitCode, stdout, stderr] = await Promise.all([
        proc.exited,
        new Response(proc.stdout).text(),
        new Response(proc.stderr).text(),
    ]);
    return { exitCode, stdout, stderr };
}

(INTEGRATION ? describe : describe.skip)("CLI integration", () => {
    let server: Awaited<ReturnType<typeof createServer>>;
    let baseUrl: string;
    let mountDir: string;
    let localDir: string;

    beforeAll(async () => {
        mountDir = await fs.mkdtemp(path.join(os.tmpdir(), "cli-inttest-mount-"));
        localDir = await fs.mkdtemp(path.join(os.tmpdir(), "cli-inttest-local-"));
        server = await createServer();
        baseUrl = await server.listen({ port: 0, host: "127.0.0.1" });
    });

    afterAll(async () => {
        await server.close();
        await fs.rm(mountDir, { recursive: true, force: true });
        await fs.rm(localDir, { recursive: true, force: true });
    });

    test("1. mount provider", async () => {
        const result = await run(
            ["providers", "mount", "inttest", "--scheme", "local", "--config", `rootDir=${mountDir}`],
            baseUrl,
        );
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("inttest");
    });

    test("2. providers list shows mount", async () => {
        const result = await run(["providers", "list"], baseUrl);
        expect(result.stdout).toContain("inttest");
    });

    test("3. ls shows empty root", async () => {
        const result = await run(["ls", "local://inttest/"], baseUrl);
        expect(result.stdout.trim()).toBe("(empty)");
    });

    test("4. upload local file via cp", async () => {
        const localFile = path.join(localDir, "hello.txt");
        await Bun.write(localFile, "hello world");

        const result = await run(["cp", localFile, "local://inttest/hello.txt"], baseUrl);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("Copied");

        const content = await fs.readFile(path.join(mountDir, "hello.txt"), "utf-8");
        expect(content).toBe("hello world");
    });

    test("5. cat file", async () => {
        const result = await run(["cat", "local://inttest/hello.txt"], baseUrl);
        expect(result.stdout).toBe("hello world");
    });

    test("6. copy remote file", async () => {
        const result = await run(["cp", "local://inttest/hello.txt", "local://inttest/hello-copy.txt"], baseUrl);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("Copied");

        const content = await fs.readFile(path.join(mountDir, "hello-copy.txt"), "utf-8");
        expect(content).toBe("hello world");
    });

    test("7. mkdir creates directory", async () => {
        const result = await run(["mkdir", "local://inttest/subdir"], baseUrl);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("Created");
    });

    test("8. ls shows created directory", async () => {
        const result = await run(["ls", "local://inttest/"], baseUrl);
        expect(result.stdout).toContain("subdir");
    });

    test("9. recursive directory copy", async () => {
        // Seed files into subdir via upload (LocalProvider.write creates parent dirs)
        const localA = path.join(localDir, "a.txt");
        const localB = path.join(localDir, "b.txt");
        await Bun.write(localA, "file a content");
        await Bun.write(localB, "file b content");

        const uploadA = await run(["cp", localA, "local://inttest/subdir/a.txt"], baseUrl);
        expect(uploadA.exitCode).toBe(0);

        const uploadB = await run(["cp", localB, "local://inttest/subdir/nested/b.txt"], baseUrl);
        expect(uploadB.exitCode).toBe(0);

        const result = await run(["cp", "local://inttest/subdir", "local://inttest/subdir2"], baseUrl);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("Copied");

        const destA = await fs.readFile(path.join(mountDir, "subdir2", "a.txt"), "utf-8");
        expect(destA).toBe("file a content");
        const destB = await fs.readFile(path.join(mountDir, "subdir2", "nested", "b.txt"), "utf-8");
        expect(destB).toBe("file b content");
    });

    test("10. delete directory", async () => {
        const result = await run(["rm", "local://inttest/subdir2"], baseUrl);
        expect(result.exitCode).toBe(0);
        await expect(fs.stat(path.join(mountDir, "subdir2"))).rejects.toThrow();
    });

    test("11. unmount provider", async () => {
        const result = await run(["providers", "unmount", "inttest"], baseUrl);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("Unmounted");
    });

    test("12. providers list no longer shows mount", async () => {
        const result = await run(["providers", "list"], baseUrl);
        expect(result.stdout).not.toContain("inttest");
    });
});
