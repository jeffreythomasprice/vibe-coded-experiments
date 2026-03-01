import { EventEmitter } from "node:events";
import { randomUUID } from "node:crypto";
import type { Operation } from "@file-manager/shared";

export type OperationType = "copy" | "move" | "upload" | "delete";
export type OperationStatus = "pending" | "in_progress" | "completed" | "failed";
export type OperationEventType = "created" | "progress" | "completed" | "failed";

export interface OperationTrackerEvents {
    created: [operation: Operation];
    progress: [operation: Operation];
    completed: [operation: Operation];
    failed: [operation: Operation];
}

function now(): string {
    return new Date().toISOString();
}

export class OperationTracker extends EventEmitter<OperationTrackerEvents> {
    private readonly operations = new Map<string, Operation>();

    create(type: OperationType, src: string, dest?: string): Operation {
        const op: Operation = {
            id: randomUUID(),
            type,
            status: "pending",
            src,
            ...(dest !== undefined ? { dest } : {}),
            totalBytes: 0,
            processedBytes: 0,
            totalFiles: 0,
            processedFiles: 0,
            createdAt: now(),
            updatedAt: now(),
        };
        this.operations.set(op.id, op);
        this.emit("created", { ...op });
        return { ...op };
    }

    start(id: string, totalFiles: number, totalBytes: number): void {
        const op = this.operations.get(id);
        if (!op) throw new Error(`Operation ${id} not found`);
        op.status = "in_progress";
        op.totalFiles = totalFiles;
        op.totalBytes = totalBytes;
        op.updatedAt = now();
        this.emit("progress", { ...op });
    }

    updateProgress(id: string, update: { processedFiles?: number; processedBytes?: number; currentFile?: string }): void {
        const op = this.operations.get(id);
        if (!op) throw new Error(`Operation ${id} not found`);
        if (update.processedFiles !== undefined) op.processedFiles = update.processedFiles;
        if (update.processedBytes !== undefined) op.processedBytes = update.processedBytes;
        if (update.currentFile !== undefined) op.currentFile = update.currentFile;
        op.updatedAt = now();
        this.emit("progress", { ...op });
    }

    complete(id: string): void {
        const op = this.operations.get(id);
        if (!op) throw new Error(`Operation ${id} not found`);
        op.status = "completed";
        delete op.currentFile;
        op.updatedAt = now();
        this.emit("completed", { ...op });
    }

    fail(id: string, error: string): void {
        const op = this.operations.get(id);
        if (!op) throw new Error(`Operation ${id} not found`);
        op.status = "failed";
        op.error = error;
        delete op.currentFile;
        op.updatedAt = now();
        this.emit("failed", { ...op });
    }

    get(id: string): Operation | undefined {
        const op = this.operations.get(id);
        return op ? { ...op } : undefined;
    }

    list(status?: OperationStatus): Operation[] {
        const ops = [...this.operations.values()];
        const filtered = status ? ops.filter((op) => op.status === status) : ops;
        return filtered.map((op) => ({ ...op }));
    }

    prune(maxAgeMs: number = 5 * 60 * 1000): number {
        const cutoff = Date.now() - maxAgeMs;
        let pruned = 0;
        for (const [id, op] of this.operations) {
            if ((op.status === "completed" || op.status === "failed") && new Date(op.updatedAt).getTime() <= cutoff) {
                this.operations.delete(id);
                pruned++;
            }
        }
        return pruned;
    }
}
