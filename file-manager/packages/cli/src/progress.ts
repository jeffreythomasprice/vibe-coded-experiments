import type { Operation } from "@file-manager/shared";
import { connectOperationSSE, getOperation } from "./api/client.js";

export function renderProgressBar(op: Operation, barWidth: number = 30): string {
    const totalFiles = op.totalFiles || 1;
    const fraction = Math.min(op.processedFiles / totalFiles, 1);
    const percent = Math.round(fraction * 100);
    const filled = Math.round(fraction * barWidth);
    const empty = barWidth - filled;

    const bar = "\u2588".repeat(filled) + "\u2591".repeat(empty);
    const filesStr = `${op.processedFiles}/${op.totalFiles} files`;
    const currentFile = op.currentFile ? `  ${basename(op.currentFile)}` : "";

    return `[${bar}] ${String(percent).padStart(3)}% ${filesStr}${currentFile}`;
}

function basename(filePath: string): string {
    const parts = filePath.split("/");
    return parts[parts.length - 1] ?? filePath;
}

function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function renderUploadBar(sent: number, total: number, barWidth: number = 30): string {
    const fraction = total > 0 ? Math.min(sent / total, 1) : 0;
    const percent = Math.round(fraction * 100);
    const filled = Math.round(fraction * barWidth);
    const empty = barWidth - filled;

    const bar = "\u2588".repeat(filled) + "\u2591".repeat(empty);
    return `[${bar}] ${String(percent).padStart(3)}% ${formatBytes(sent)}/${formatBytes(total)}`;
}

export function trackUploadStream(
    stream: ReadableStream<Uint8Array>,
    totalBytes: number,
): ReadableStream<Uint8Array> {
    let sent = 0;
    return new ReadableStream({
        async start(controller) {
            const reader = stream.getReader();
            try {
                while (true) {
                    const { done, value } = await reader.read();
                    if (done) break;
                    sent += value.byteLength;
                    controller.enqueue(value);
                    process.stderr.write(`\r${renderUploadBar(sent, totalBytes)}`);
                }
                process.stderr.write(`\r${renderUploadBar(sent, totalBytes)}\n`);
                controller.close();
            } catch (err) {
                process.stderr.write("\n");
                controller.error(err);
            }
        },
    });
}

export async function trackOperation(operationId: string, timeoutMs: number = 300_000): Promise<Operation> {
    // Check if the operation already completed before we connect to SSE
    const current = await getOperation(operationId);
    if (current.status === "completed") {
        process.stderr.write(`${renderProgressBar(current)}\n`);
        return current;
    }
    if (current.status === "failed") {
        throw new Error(current.error ?? "Operation failed");
    }

    return new Promise((resolve, reject) => {
        let settled = false;

        const settle = () => {
            settled = true;
            sse.abort();
            clearTimeout(timer);
        };

        const sse = connectOperationSSE({
            onProgress(op) {
                if (op.id !== operationId) return;
                process.stderr.write(`\r${renderProgressBar(op)}`);
            },
            onCompleted(op) {
                if (op.id !== operationId || settled) return;
                process.stderr.write(`\r${renderProgressBar(op)}\n`);
                settle();
                resolve(op);
            },
            onFailed(op) {
                if (op.id !== operationId || settled) return;
                process.stderr.write("\n");
                settle();
                reject(new Error(op.error ?? "Operation failed"));
            },
        });

        // Re-check status after SSE connects to close the race window
        void getOperation(operationId).then((op) => {
            if (settled) return;
            if (op.status === "completed") {
                process.stderr.write(`\r${renderProgressBar(op)}\n`);
                settle();
                resolve(op);
            } else if (op.status === "failed") {
                process.stderr.write("\n");
                settle();
                reject(new Error(op.error ?? "Operation failed"));
            }
        });

        const timer = setTimeout(() => {
            if (settled) return;
            settle();
            reject(new Error(`Operation ${operationId} timed out after ${timeoutMs}ms`));
        }, timeoutMs);
    });
}
