import type { FastifyPluginAsync } from "fastify";
import {
    type MountInfo,
    mountInfoSchema,
    assertLocalProviderConfig,
} from "@file-manager/shared";
import { LocalProvider } from "../providers/local.js";

export const providerRoutes: FastifyPluginAsync = async (fastify) => {
    // GET /api/v1/providers
    fastify.get("/providers", {
        schema: { response: { 200: { type: "array", items: mountInfoSchema } } },
    }, async (_req, reply) => {
        const mounts = fastify.registry.list().map(({ mountId, scheme, config }) => ({
            mountId,
            scheme,
            config,
        }));
        return reply.send(mounts);
    });

    // POST /api/v1/providers
    fastify.post<{ Body: MountInfo }>("/providers", {
        schema: { body: mountInfoSchema, response: { 201: mountInfoSchema } },
    }, async (req, reply) => {
        const { mountId, scheme, config } = req.body;

        let provider;
        if (scheme === "local") {
            try {
                assertLocalProviderConfig(config);
            } catch (err) {
                return reply.badRequest((err as Error).message);
            }
            provider = new LocalProvider(config.rootDir);
        } else {
            return reply.badRequest(`Unsupported scheme: ${scheme}`);
        }

        try {
            fastify.registry.mount(mountId, scheme, config, provider);
        } catch (err) {
            return reply.conflict((err as Error).message);
        }

        return reply.code(201).send({ mountId, scheme, config });
    });

    // DELETE /api/v1/providers/:mountId
    fastify.delete<{ Params: { mountId: string } }>("/providers/:mountId", {
        schema: {
            params: {
                type: "object",
                properties: { mountId: { type: "string" } },
                required: ["mountId"],
            },
        },
    }, async (req, reply) => {
        const { mountId } = req.params;
        const removed = fastify.registry.unmount(mountId);
        if (!removed) {
            return reply.notFound(`Provider '${mountId}' not found`);
        }
        return reply.code(204).send();
    });
};
