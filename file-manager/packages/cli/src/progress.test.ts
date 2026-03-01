import { describe, test, expect } from "bun:test";
import { renderProgressBar, renderUploadBar, trackUploadStream } from "./progress.js";
import type { Operation } from "@file-manager/shared";

function makeOp(overrides: Partial<Operation> = {}): Operation {
    return {
        id: "test-id",
        type: "copy",
        status: "in_progress",
        src: "local://a/file.txt",
        dest: "local://b/file.txt",
        totalBytes: 1000,
        processedBytes: 0,
        totalFiles: 10,
        processedFiles: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        ...overrides,
    };
}

describe("renderProgressBar", () => {
    test("shows 0% for no progress", () => {
        const result = renderProgressBar(makeOp(), 10);
        expect(result).toContain("  0%");
        expect(result).toContain("0/10 files");
        expect(result).toContain("\u2591".repeat(10));
    });

    test("shows 50% for half progress", () => {
        const result = renderProgressBar(makeOp({ processedFiles: 5 }), 10);
        expect(result).toContain(" 50%");
        expect(result).toContain("5/10 files");
        expect(result).toContain("\u2588".repeat(5));
    });

    test("shows 100% when all files processed", () => {
        const result = renderProgressBar(makeOp({ processedFiles: 10 }), 10);
        expect(result).toContain("100%");
        expect(result).toContain("10/10 files");
        expect(result).toContain("\u2588".repeat(10));
    });

    test("shows current file basename", () => {
        const result = renderProgressBar(makeOp({ processedFiles: 3, currentFile: "/some/deep/path/report.pdf" }), 10);
        expect(result).toContain("report.pdf");
    });

    test("does not exceed 100%", () => {
        const result = renderProgressBar(makeOp({ processedFiles: 15, totalFiles: 10 }), 10);
        expect(result).toContain("100%");
        expect(result).toContain("\u2588".repeat(10));
    });

    test("handles totalFiles of 0 gracefully", () => {
        const result = renderProgressBar(makeOp({ totalFiles: 0, processedFiles: 0 }), 10);
        expect(result).toContain("0%");
    });
});

describe("renderUploadBar", () => {
    test("shows 0% at start", () => {
        const result = renderUploadBar(0, 1024, 10);
        expect(result).toContain("  0%");
        expect(result).toContain("0 B/1.0 KB");
        expect(result).toContain("\u2591".repeat(10));
    });

    test("shows 50% at halfway", () => {
        const result = renderUploadBar(512, 1024, 10);
        expect(result).toContain(" 50%");
        expect(result).toContain("\u2588".repeat(5));
    });

    test("shows 100% when complete", () => {
        const result = renderUploadBar(1024, 1024, 10);
        expect(result).toContain("100%");
        expect(result).toContain("1.0 KB/1.0 KB");
        expect(result).toContain("\u2588".repeat(10));
    });

    test("formats MB correctly", () => {
        const mb = 5 * 1024 * 1024;
        const result = renderUploadBar(mb, mb, 10);
        expect(result).toContain("5.0 MB/5.0 MB");
    });

    test("formats GB correctly", () => {
        const gb = 2 * 1024 * 1024 * 1024;
        const result = renderUploadBar(gb, gb, 10);
        expect(result).toContain("2.0 GB/2.0 GB");
    });

    test("handles 0 total gracefully", () => {
        const result = renderUploadBar(0, 0, 10);
        expect(result).toContain("  0%");
    });
});

describe("trackUploadStream", () => {
    test("passes through all data unchanged", async () => {
        const input = new Uint8Array([1, 2, 3, 4, 5]);
        const source = new ReadableStream<Uint8Array>({
            start(controller) {
                controller.enqueue(input);
                controller.close();
            },
        });

        const tracked = trackUploadStream(source, input.byteLength);
        const reader = tracked.getReader();
        const chunks: Uint8Array[] = [];

        while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            chunks.push(value);
        }

        expect(chunks).toHaveLength(1);
        expect(chunks[0]).toEqual(input);
    });

    test("passes through multiple chunks", async () => {
        const chunk1 = new Uint8Array([1, 2, 3]);
        const chunk2 = new Uint8Array([4, 5]);
        const source = new ReadableStream<Uint8Array>({
            start(controller) {
                controller.enqueue(chunk1);
                controller.enqueue(chunk2);
                controller.close();
            },
        });

        const tracked = trackUploadStream(source, 5);
        const reader = tracked.getReader();
        const chunks: Uint8Array[] = [];

        while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            chunks.push(value);
        }

        expect(chunks).toHaveLength(2);
        expect(chunks[0]).toEqual(chunk1);
        expect(chunks[1]).toEqual(chunk2);
    });
});
