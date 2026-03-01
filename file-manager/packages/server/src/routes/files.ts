import type { FastifyPluginAsync } from "fastify";
import { Readable } from "node:stream";
import { type MoveRequest, moveRequestSchema, srcDestRequestSchema } from "@file-manager/shared";
import type { StorageProvider } from "@file-manager/shared";

type SrcDestRequest = MoveRequest; // { src, dest } — body shape shared by move and copy routes

interface FilesParams {
    mountId: string;
    "*": string;
}

async function* readableToAsyncIterable(stream: Readable): AsyncIterable<Buffer> {
    for await (const chunk of stream) {
        yield Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk as Uint8Array);
    }
}

export function parseFileUri(uri: string): { mountId: string; path: string } | null {
    // Expected format: <mountId>://<path>
    const match = uri.match(/^([a-zA-Z0-9_-]+):\/\/?(.*)/);
    if (!match) return null;
    const mountId = match[1];
    const path = "/" + (match[2] ?? "");
    if (!mountId) return null;
    return { mountId, path };
}

export interface ScanResult {
    totalFiles: number;
    totalBytes: number;
}

export async function scanRecursive(
    provider: StorageProvider,
    path: string,
): Promise<ScanResult> {
    const stat = await provider.stat(path);
    if (stat.type === "directory") {
        const entries = await provider.list(path);
        let totalFiles = 0;
        let totalBytes = 0;
        for (const entry of entries) {
            const child = await scanRecursive(provider, entry.path);
            totalFiles += child.totalFiles;
            totalBytes += child.totalBytes;
        }
        return { totalFiles, totalBytes };
    }
    return { totalFiles: 1, totalBytes: stat.size };
}

export interface CopyProgressCallbacks {
    onFileStart?: (path: string) => void;
    onFileComplete?: (path: string, bytes: number) => void;
}

export async function copyRecursive(
    srcProvider: StorageProvider,
    srcPath: string,
    destProvider: StorageProvider,
    destPath: string,
    callbacks?: CopyProgressCallbacks,
): Promise<void> {
    const srcStat = await srcProvider.stat(srcPath);
    if (srcStat.type === "directory") {
        await destProvider.mkdir(destPath);
        const entries = await srcProvider.list(srcPath);
        for (const entry of entries) {
            const destEntryPath = `${destPath}/${entry.name}`;
            await copyRecursive(srcProvider, entry.path, destProvider, destEntryPath, callbacks);
        }
    } else {
        callbacks?.onFileStart?.(srcPath);
        await destProvider.write(destPath, srcProvider.read(srcPath));
        callbacks?.onFileComplete?.(srcPath, srcStat.size);
    }
}

export const fileRoutes: FastifyPluginAsync = async (fastify) => {
    // GET /api/v1/files/:mountId/*path
    fastify.get<{ Params: FilesParams; Querystring: { stat?: string } }>("/files/:mountId/*", async (req, reply) => {
        const { mountId } = req.params;
        const filePath = "/" + (req.params["*"] ?? "");
        const wantStat = "stat" in req.query;

        const mount = fastify.registry.get(mountId);
        if (!mount) {
            return reply.notFound(`Provider '${mountId}' not found`);
        }

        const { provider } = mount;

        if (wantStat) {
            try {
                const stat = await provider.stat(filePath);
                return reply.send(stat);
            } catch (err) {
                return reply.notFound((err as Error).message);
            }
        }

        let stat;
        try {
            stat = await provider.stat(filePath);
        } catch (err) {
            return reply.notFound((err as Error).message);
        }

        if (stat.type === "directory") {
            const entries = await provider.list(filePath);
            return reply.send(entries);
        }

        // Stream file bytes
        reply.header("Content-Type", "application/octet-stream");
        reply.header("Content-Length", stat.size);

        const readable = Readable.from(provider.read(filePath));
        return reply.send(readable);
    });

    // POST /api/v1/files/:mountId/*path — write file
    fastify.post<{ Params: FilesParams; Body: Buffer }>("/files/:mountId/*", {
        bodyLimit: 1024 * 1024 * 1024, // 1 GB — default 1MB is too small for file uploads
        schema: {
            response: {
                201: {
                    type: "object",
                    properties: { path: { type: "string" }, operationId: { type: "string" } },
                    required: ["path", "operationId"],
                    additionalProperties: false,
                },
            },
        },
    }, async (req, reply) => {
        const { mountId } = req.params;
        const filePath = "/" + (req.params["*"] ?? "");

        const mount = fastify.registry.get(mountId);
        if (!mount) {
            return reply.notFound(`Provider '${mountId}' not found`);
        }

        const { provider } = mount;
        // Content-type parser passes raw Readable stream (see server.ts), not a Buffer
        const bodyStream = req.body as unknown as Readable;

        const tracker = fastify.tracker;
        const src = `${mountId}://${filePath.replace(/^\//, "")}`;
        const op = tracker.create("upload", src);
        const contentLength = req.headers["content-length"];
        const totalBytes = contentLength ? parseInt(contentLength, 10) : 0;
        tracker.start(op.id, 1, totalBytes);

        try {
            async function* trackUploadProgress(stream: AsyncIterable<Buffer>): AsyncIterable<Buffer> {
                let processedBytes = 0;
                for await (const chunk of stream) {
                    processedBytes += chunk.length;
                    tracker.updateProgress(op.id, { processedBytes, currentFile: filePath });
                    yield chunk;
                }
            }

            await provider.write(filePath, trackUploadProgress(readableToAsyncIterable(bodyStream)));
            tracker.updateProgress(op.id, { processedFiles: 1 });
            tracker.complete(op.id);
        } catch (err) {
            tracker.fail(op.id, (err as Error).message);
            return reply.internalServerError((err as Error).message);
        }

        return reply.code(201).send({ path: filePath, operationId: op.id });
    });

    // PUT /api/v1/files/:mountId/*path — create directory
    fastify.put<{ Params: FilesParams }>("/files/:mountId/*", {
        schema: {
            response: {
                201: { type: "object", properties: { path: { type: "string" } }, required: ["path"], additionalProperties: false },
            },
        },
    }, async (req, reply) => {
        const { mountId } = req.params;
        const filePath = "/" + (req.params["*"] ?? "");

        const mount = fastify.registry.get(mountId);
        if (!mount) {
            return reply.notFound(`Provider '${mountId}' not found`);
        }

        try {
            await mount.provider.mkdir(filePath);
        } catch (err) {
            return reply.internalServerError((err as Error).message);
        }

        return reply.code(201).send({ path: filePath });
    });

    // DELETE /api/v1/files/:mountId/*path
    fastify.delete<{ Params: FilesParams }>("/files/:mountId/*", {
        schema: {
            response: {
                200: {
                    type: "object",
                    properties: { operationId: { type: "string" } },
                    required: ["operationId"],
                    additionalProperties: false,
                },
            },
        },
    }, async (req, reply) => {
        const { mountId } = req.params;
        const filePath = "/" + (req.params["*"] ?? "");

        const mount = fastify.registry.get(mountId);
        if (!mount) {
            return reply.notFound(`Provider '${mountId}' not found`);
        }

        const tracker = fastify.tracker;
        const src = `${mountId}://${filePath.replace(/^\//, "")}`;
        const op = tracker.create("delete", src);
        tracker.start(op.id, 1, 0);

        try {
            await mount.provider.delete(filePath);
            tracker.updateProgress(op.id, { processedFiles: 1 });
            tracker.complete(op.id);
        } catch (err) {
            tracker.fail(op.id, (err as Error).message);
            return reply.notFound((err as Error).message);
        }

        return reply.code(200).send({ operationId: op.id });
    });

    // POST /api/v1/files/move
    const moveResponseSchema = {
        type: "object",
        properties: {
            src: { type: "string" },
            dest: { type: "string" },
            operationId: { type: "string" },
        },
        required: ["src", "dest"],
        additionalProperties: false,
    } as const;

    fastify.post<{ Body: MoveRequest }>("/files/move", {
        schema: { body: moveRequestSchema, response: { 200: moveResponseSchema } },
    }, async (req, reply) => {
        const { src, dest } = req.body;

        const srcParsed = parseFileUri(src);
        const destParsed = parseFileUri(dest);

        if (!srcParsed || !destParsed) {
            return reply.badRequest("Invalid URI format. Expected: <mountId>://<path>");
        }

        const srcMount = fastify.registry.get(srcParsed.mountId);
        const destMount = fastify.registry.get(destParsed.mountId);

        if (!srcMount) {
            return reply.notFound(`Provider '${srcParsed.mountId}' not found`);
        }
        if (!destMount) {
            return reply.notFound(`Provider '${destParsed.mountId}' not found`);
        }

        let operationId: string | undefined;

        if (srcParsed.mountId === destParsed.mountId) {
            // Same mount — use native move (instant but tracked)
            const tracker = fastify.tracker;
            const op = tracker.create("move", src, dest);
            operationId = op.id;
            tracker.start(op.id, 1, 0);

            try {
                await srcMount.provider.move(srcParsed.path, destParsed.path);
                tracker.updateProgress(op.id, { processedFiles: 1 });
                tracker.complete(op.id);
            } catch (err) {
                tracker.fail(op.id, (err as Error).message);
                return reply.internalServerError((err as Error).message);
            }
        } else {
            // Cross-mount — tracked recursive copy → delete source
            const tracker = fastify.tracker;
            const op = tracker.create("move", src, dest);
            operationId = op.id;

            // Run in background so the client can connect to SSE before it starts
            const runMove = async () => {
                try {
                    const scan = await scanRecursive(srcMount.provider, srcParsed.path);
                    tracker.start(op.id, scan.totalFiles, scan.totalBytes);

                    let processedFiles = 0;
                    let processedBytes = 0;
                    await copyRecursive(srcMount.provider, srcParsed.path, destMount.provider, destParsed.path, {
                        onFileStart(path) {
                            tracker.updateProgress(op.id, { currentFile: path });
                        },
                        onFileComplete(_path, bytes) {
                            processedFiles++;
                            processedBytes += bytes;
                            tracker.updateProgress(op.id, { processedFiles, processedBytes });
                        },
                    });

                    await srcMount.provider.delete(srcParsed.path);
                    tracker.complete(op.id);
                } catch (err) {
                    tracker.fail(op.id, (err as Error).message);
                }
            };
            void runMove();
        }

        return reply.send({ src, dest, operationId });
    });

    // POST /api/v1/files/copy
    const copyResponseSchema = {
        type: "object",
        properties: {
            src: { type: "string" },
            dest: { type: "string" },
            operationId: { type: "string" },
        },
        required: ["src", "dest", "operationId"],
        additionalProperties: false,
    } as const;

    fastify.post<{ Body: SrcDestRequest }>("/files/copy", {
        schema: { body: srcDestRequestSchema, response: { 202: copyResponseSchema } },
    }, async (req, reply) => {
        const { src, dest } = req.body;

        const srcParsed = parseFileUri(src);
        const destParsed = parseFileUri(dest);

        if (!srcParsed || !destParsed) {
            return reply.badRequest("Invalid URI format. Expected: <mountId>://<path>");
        }

        const srcMount = fastify.registry.get(srcParsed.mountId);
        const destMount = fastify.registry.get(destParsed.mountId);

        if (!srcMount) {
            return reply.notFound(`Provider '${srcParsed.mountId}' not found`);
        }
        if (!destMount) {
            return reply.notFound(`Provider '${destParsed.mountId}' not found`);
        }

        const tracker = fastify.tracker;
        const op = tracker.create("copy", src, dest);

        // Run the copy in the background so the client can connect to SSE before it starts
        const runCopy = async () => {
            try {
                const scan = await scanRecursive(srcMount.provider, srcParsed.path);
                tracker.start(op.id, scan.totalFiles, scan.totalBytes);

                let processedFiles = 0;
                let processedBytes = 0;
                await copyRecursive(srcMount.provider, srcParsed.path, destMount.provider, destParsed.path, {
                    onFileStart(path) {
                        tracker.updateProgress(op.id, { currentFile: path });
                    },
                    onFileComplete(_path, bytes) {
                        processedFiles++;
                        processedBytes += bytes;
                        tracker.updateProgress(op.id, { processedFiles, processedBytes });
                    },
                });

                tracker.complete(op.id);
            } catch (err) {
                tracker.fail(op.id, (err as Error).message);
            }
        };
        void runCopy();

        return reply.code(202).send({ src, dest, operationId: op.id });
    });
};
