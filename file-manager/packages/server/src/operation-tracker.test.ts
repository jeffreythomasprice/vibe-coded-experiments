import { describe, test, expect } from "bun:test";
import { OperationTracker } from "./operation-tracker.js";
import type { Operation } from "@file-manager/shared";

describe("OperationTracker", () => {
    // ── create ────────────────────────────────────────────────────────────────

    test("create() returns an operation with pending status", () => {
        const tracker = new OperationTracker();
        const op = tracker.create("copy", "local://a/file.txt", "local://b/file.txt");

        expect(op.id).toBeDefined();
        expect(op.type).toBe("copy");
        expect(op.status).toBe("pending");
        expect(op.src).toBe("local://a/file.txt");
        expect(op.dest).toBe("local://b/file.txt");
        expect(op.totalFiles).toBe(0);
        expect(op.processedFiles).toBe(0);
        expect(op.totalBytes).toBe(0);
        expect(op.processedBytes).toBe(0);
    });

    test("create() allows omitting dest for upload/delete operations", () => {
        const tracker = new OperationTracker();
        const upload = tracker.create("upload", "local://a/file.txt");
        expect(upload.type).toBe("upload");
        expect(upload.src).toBe("local://a/file.txt");
        expect(upload.dest).toBeUndefined();

        const del = tracker.create("delete", "local://a/file.txt");
        expect(del.type).toBe("delete");
        expect(del.dest).toBeUndefined();
    });

    test("create() emits 'created' event", () => {
        const tracker = new OperationTracker();
        const events: Operation[] = [];
        tracker.on("created", (op) => events.push(op));

        tracker.create("copy", "local://a/x", "local://b/x");

        expect(events).toHaveLength(1);
        expect(events[0]!.status).toBe("pending");
    });

    // ── start ─────────────────────────────────────────────────────────────────

    test("start() sets status to in_progress and sets totals", () => {
        const tracker = new OperationTracker();
        const op = tracker.create("copy", "a", "b");

        tracker.start(op.id, 10, 5000);

        const updated = tracker.get(op.id)!;
        expect(updated.status).toBe("in_progress");
        expect(updated.totalFiles).toBe(10);
        expect(updated.totalBytes).toBe(5000);
    });

    test("start() emits 'progress' event", () => {
        const tracker = new OperationTracker();
        const events: Operation[] = [];
        tracker.on("progress", (op) => events.push(op));

        const op = tracker.create("move", "a", "b");
        tracker.start(op.id, 5, 1000);

        expect(events).toHaveLength(1);
        expect(events[0]!.status).toBe("in_progress");
    });

    test("start() throws for unknown id", () => {
        const tracker = new OperationTracker();
        expect(() => tracker.start("nonexistent", 0, 0)).toThrow("not found");
    });

    // ── updateProgress ────────────────────────────────────────────────────────

    test("updateProgress() updates fields and emits 'progress'", () => {
        const tracker = new OperationTracker();
        const events: Operation[] = [];
        tracker.on("progress", (op) => events.push(op));

        const op = tracker.create("copy", "a", "b");
        tracker.start(op.id, 10, 5000);
        tracker.updateProgress(op.id, { processedFiles: 3, processedBytes: 1500, currentFile: "/foo.txt" });

        const updated = tracker.get(op.id)!;
        expect(updated.processedFiles).toBe(3);
        expect(updated.processedBytes).toBe(1500);
        expect(updated.currentFile).toBe("/foo.txt");
        // start + updateProgress = 2 progress events
        expect(events).toHaveLength(2);
    });

    test("updateProgress() allows partial updates", () => {
        const tracker = new OperationTracker();
        const op = tracker.create("copy", "a", "b");
        tracker.start(op.id, 5, 1000);

        tracker.updateProgress(op.id, { processedFiles: 1 });
        const updated = tracker.get(op.id)!;
        expect(updated.processedFiles).toBe(1);
        expect(updated.processedBytes).toBe(0); // unchanged
    });

    // ── complete ──────────────────────────────────────────────────────────────

    test("complete() sets status to completed and clears currentFile", () => {
        const tracker = new OperationTracker();
        const op = tracker.create("copy", "a", "b");
        tracker.start(op.id, 1, 100);
        tracker.updateProgress(op.id, { currentFile: "/x.txt" });
        tracker.complete(op.id);

        const updated = tracker.get(op.id)!;
        expect(updated.status).toBe("completed");
        expect(updated.currentFile).toBeUndefined();
    });

    test("complete() emits 'completed' event", () => {
        const tracker = new OperationTracker();
        const events: Operation[] = [];
        tracker.on("completed", (op) => events.push(op));

        const op = tracker.create("copy", "a", "b");
        tracker.complete(op.id);

        expect(events).toHaveLength(1);
        expect(events[0]!.status).toBe("completed");
    });

    // ── fail ──────────────────────────────────────────────────────────────────

    test("fail() sets status to failed with error message", () => {
        const tracker = new OperationTracker();
        const op = tracker.create("move", "a", "b");
        tracker.fail(op.id, "disk full");

        const updated = tracker.get(op.id)!;
        expect(updated.status).toBe("failed");
        expect(updated.error).toBe("disk full");
        expect(updated.currentFile).toBeUndefined();
    });

    test("fail() emits 'failed' event", () => {
        const tracker = new OperationTracker();
        const events: Operation[] = [];
        tracker.on("failed", (op) => events.push(op));

        const op = tracker.create("copy", "a", "b");
        tracker.fail(op.id, "oops");

        expect(events).toHaveLength(1);
        expect(events[0]!.error).toBe("oops");
    });

    // ── get ───────────────────────────────────────────────────────────────────

    test("get() returns undefined for unknown id", () => {
        const tracker = new OperationTracker();
        expect(tracker.get("nonexistent")).toBeUndefined();
    });

    test("get() returns a snapshot (not a reference)", () => {
        const tracker = new OperationTracker();
        const op = tracker.create("copy", "a", "b");
        const snapshot = tracker.get(op.id)!;
        snapshot.status = "failed";

        // Original should be unchanged
        expect(tracker.get(op.id)!.status).toBe("pending");
    });

    // ── list ──────────────────────────────────────────────────────────────────

    test("list() returns all operations", () => {
        const tracker = new OperationTracker();
        tracker.create("copy", "a", "b");
        tracker.create("move", "c", "d");

        expect(tracker.list()).toHaveLength(2);
    });

    test("list() filters by status", () => {
        const tracker = new OperationTracker();
        const op1 = tracker.create("copy", "a", "b");
        tracker.create("move", "c", "d");
        tracker.complete(op1.id);

        expect(tracker.list("completed")).toHaveLength(1);
        expect(tracker.list("pending")).toHaveLength(1);
        expect(tracker.list("in_progress")).toHaveLength(0);
    });

    // ── prune ─────────────────────────────────────────────────────────────────

    test("prune() removes completed/failed ops older than maxAge", () => {
        const tracker = new OperationTracker();
        const op = tracker.create("copy", "a", "b");
        tracker.complete(op.id);

        // With maxAge=0, everything completed is "old enough"
        const pruned = tracker.prune(0);
        expect(pruned).toBe(1);
        expect(tracker.list()).toHaveLength(0);
    });

    test("prune() keeps pending and in_progress ops", () => {
        const tracker = new OperationTracker();
        const op1 = tracker.create("copy", "a", "b");
        tracker.create("move", "c", "d"); // stays pending
        tracker.start(op1.id, 1, 1); // now in_progress

        const pruned = tracker.prune(0);
        expect(pruned).toBe(0);
        expect(tracker.list()).toHaveLength(2);
    });

    test("prune() keeps recently completed ops when maxAge is large", () => {
        const tracker = new OperationTracker();
        const op = tracker.create("copy", "a", "b");
        tracker.complete(op.id);

        // Default maxAge is 5 minutes — just completed should survive
        const pruned = tracker.prune();
        expect(pruned).toBe(0);
        expect(tracker.list()).toHaveLength(1);
    });
});
