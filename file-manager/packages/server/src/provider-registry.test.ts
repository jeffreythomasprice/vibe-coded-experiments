import { describe, test, expect, beforeEach } from "bun:test";
import { ProviderRegistry } from "./provider-registry.js";
import type { StorageProvider } from "@file-manager/shared";

function makeProvider(): StorageProvider {
    return {} as StorageProvider;
}

describe("ProviderRegistry", () => {
    let registry: ProviderRegistry;

    beforeEach(() => {
        registry = new ProviderRegistry();
    });

    test("mount() registers a provider and it appears in list()", async () => {
        await registry.mount("docs", "local", { rootDir: "/tmp/docs" }, makeProvider());
        const mounts = registry.list();
        expect(mounts).toHaveLength(1);
        expect(mounts[0]!.mountId).toBe("docs");
        expect(mounts[0]!.scheme).toBe("local");
    });

    test("mount() throws on duplicate mountId", async () => {
        await registry.mount("docs", "local", { rootDir: "/tmp/docs" }, makeProvider());
        await expect(registry.mount("docs", "local", { rootDir: "/tmp/other" }, makeProvider())).rejects.toThrow("Mount ID 'docs' is already in use");
    });

    test("get() returns the mount when found", async () => {
        const provider = makeProvider();
        await registry.mount("docs", "local", { rootDir: "/tmp/docs" }, provider);
        const mount = registry.get("docs");
        expect(mount).toBeDefined();
        expect(mount!.mountId).toBe("docs");
        expect(mount!.provider).toBe(provider);
    });

    test("get() returns undefined when not found", () => {
        expect(registry.get("nonexistent")).toBeUndefined();
    });

    test("unmount() returns true when found and removes it", async () => {
        await registry.mount("docs", "local", { rootDir: "/tmp/docs" }, makeProvider());
        expect(await registry.unmount("docs")).toBe(true);
        expect(registry.get("docs")).toBeUndefined();
        expect(registry.list()).toHaveLength(0);
    });

    test("unmount() returns false when not found", async () => {
        expect(await registry.unmount("nonexistent")).toBe(false);
    });

    test("list() returns all mounts as array", async () => {
        await registry.mount("a", "local", {}, makeProvider());
        await registry.mount("b", "local", {}, makeProvider());
        await registry.mount("c", "local", {}, makeProvider());
        const mounts = registry.list();
        expect(mounts).toHaveLength(3);
        const ids = mounts.map((m) => m.mountId);
        expect(ids).toContain("a");
        expect(ids).toContain("b");
        expect(ids).toContain("c");
    });

    test("list() returns empty array on fresh registry", () => {
        expect(registry.list()).toHaveLength(0);
    });
});
