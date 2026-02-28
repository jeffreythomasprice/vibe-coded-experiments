import type { FastifyPluginAsync } from "fastify";
import type { ProviderScheme } from "@file-manager/shared";
import { LocalProvider } from "../providers/local.js";

interface MountBody {
    mountId: string;
    scheme: ProviderScheme;
    config: Record<string, string>;
}

export const providerRoutes: FastifyPluginAsync = async (fastify) => {
    // GET /api/v1/providers
    fastify.get("/providers", async (_req, reply) => {
        const mounts = fastify.registry.list().map(({ mountId, scheme, config }) => ({
            mountId,
            scheme,
            config,
        }));
        return reply.send(mounts);
    });

    // POST /api/v1/providers
    fastify.post<{ Body: MountBody }>("/providers", async (req, reply) => {
        const { mountId, scheme, config } = req.body;

        if (!mountId || !scheme || !config) {
            return reply.badRequest("mountId, scheme, and config are required");
        }

        let provider;
        if (scheme === "local") {
            const rootDir = config["rootDir"];
            if (!rootDir) {
                return reply.badRequest("config.rootDir is required for local provider");
            }
            provider = new LocalProvider(rootDir);
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
    fastify.delete<{ Params: { mountId: string } }>("/providers/:mountId", async (req, reply) => {
        const { mountId } = req.params;
        const removed = fastify.registry.unmount(mountId);
        if (!removed) {
            return reply.notFound(`Provider '${mountId}' not found`);
        }
        return reply.code(204).send();
    });
};
