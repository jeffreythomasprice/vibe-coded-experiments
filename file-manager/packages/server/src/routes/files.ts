import type { FastifyPluginAsync } from "fastify";
import { type MoveRequest, moveRequestSchema, srcDestRequestSchema } from "@file-manager/shared";

type SrcDestRequest = MoveRequest; // { src, dest } — body shape shared by move and copy routes

interface FilesParams {
    mountId: string;
    "*": string;
}

async function* bufferToAsyncIterable(buf: Buffer): AsyncIterable<Buffer> {
    yield buf;
}

function parseFileUri(uri: string): { mountId: string; path: string } | null {
    // Expected format: <scheme>://<mountId>/<path>
    const match = uri.match(/^[a-z]+:\/\/([^/]+)\/?(.*)/);
    if (!match) return null;
    const mountId = match[1];
    const path = "/" + (match[2] ?? "");
    if (!mountId) return null;
    return { mountId, path };
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

        const chunks: Buffer[] = [];
        for await (const chunk of provider.read(filePath)) {
            chunks.push(chunk);
        }
        return reply.send(Buffer.concat(chunks));
    });

    // POST /api/v1/files/:mountId/*path — write file
    fastify.post<{ Params: FilesParams; Body: Buffer }>("/files/:mountId/*", {
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

        const { provider } = mount;
        const body = req.body ?? Buffer.alloc(0);

        try {
            await provider.write(filePath, bufferToAsyncIterable(Buffer.isBuffer(body) ? body : Buffer.from(body)));
        } catch (err) {
            return reply.internalServerError((err as Error).message);
        }

        return reply.code(201).send({ path: filePath });
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
        schema: { response: { 204: { type: "null" } } },
    }, async (req, reply) => {
        const { mountId } = req.params;
        const filePath = "/" + (req.params["*"] ?? "");

        const mount = fastify.registry.get(mountId);
        if (!mount) {
            return reply.notFound(`Provider '${mountId}' not found`);
        }

        try {
            await mount.provider.delete(filePath);
        } catch (err) {
            return reply.notFound((err as Error).message);
        }

        return reply.code(204).send();
    });

    // POST /api/v1/files/move
    fastify.post<{ Body: MoveRequest }>("/files/move", {
        schema: { body: moveRequestSchema, response: { 200: moveRequestSchema } },
    }, async (req, reply) => {
        const { src, dest } = req.body;

        const srcParsed = parseFileUri(src);
        const destParsed = parseFileUri(dest);

        if (!srcParsed || !destParsed) {
            return reply.badRequest("Invalid URI format. Expected: <scheme>://<mountId>/<path>");
        }

        const srcMount = fastify.registry.get(srcParsed.mountId);
        const destMount = fastify.registry.get(destParsed.mountId);

        if (!srcMount) {
            return reply.notFound(`Provider '${srcParsed.mountId}' not found`);
        }
        if (!destMount) {
            return reply.notFound(`Provider '${destParsed.mountId}' not found`);
        }

        if (srcParsed.mountId === destParsed.mountId) {
            // Same mount — use native move
            try {
                await srcMount.provider.move(srcParsed.path, destParsed.path);
            } catch (err) {
                return reply.internalServerError((err as Error).message);
            }
        } else {
            // Cross-mount — stream read → write → delete
            try {
                await destMount.provider.write(destParsed.path, srcMount.provider.read(srcParsed.path));
                await srcMount.provider.delete(srcParsed.path);
            } catch (err) {
                return reply.internalServerError((err as Error).message);
            }
        }

        return reply.send({ src, dest });
    });

    // POST /api/v1/files/copy
    fastify.post<{ Body: SrcDestRequest }>("/files/copy", {
        schema: { body: srcDestRequestSchema, response: { 200: srcDestRequestSchema } },
    }, async (req, reply) => {
        const { src, dest } = req.body;

        const srcParsed = parseFileUri(src);
        const destParsed = parseFileUri(dest);

        if (!srcParsed || !destParsed) {
            return reply.badRequest("Invalid URI format. Expected: <scheme>://<mountId>/<path>");
        }

        const srcMount = fastify.registry.get(srcParsed.mountId);
        const destMount = fastify.registry.get(destParsed.mountId);

        if (!srcMount) {
            return reply.notFound(`Provider '${srcParsed.mountId}' not found`);
        }
        if (!destMount) {
            return reply.notFound(`Provider '${destParsed.mountId}' not found`);
        }

        try {
            await destMount.provider.write(destParsed.path, srcMount.provider.read(srcParsed.path));
        } catch (err) {
            return reply.internalServerError((err as Error).message);
        }

        return reply.send({ src, dest });
    });
};
