import Fastify from "fastify";
import sensible from "@fastify/sensible";
import cors from "@fastify/cors";
import { ProviderRegistry } from "./provider-registry.js";
import { OperationTracker } from "./operation-tracker.js";
import { providerRoutes } from "./routes/providers.js";
import { fileRoutes } from "./routes/files.js";
import { operationRoutes } from "./routes/operations.js";
import type { Db } from "./db/client.js";

export interface CreateServerOptions {
    db?: Db;
}

export async function createServer(options: CreateServerOptions = {}) {
    const fastify = Fastify({
        logger: true,
    });

    await fastify.register(sensible);
    await fastify.register(cors, { origin: true });

    // Accept raw binary uploads — pass the stream through without buffering
    fastify.addContentTypeParser("application/octet-stream", function (_req, payload, done) {
        done(null, payload);
    });

    const registry = new ProviderRegistry();
    if (options.db !== undefined) {
        await registry.initialize(options.db);
    }

    const tracker = new OperationTracker();

    // Make registry and tracker available to routes via decoration
    fastify.decorate("registry", registry);
    fastify.decorate("tracker", tracker);

    await fastify.register(providerRoutes, { prefix: "/api/v1" });
    await fastify.register(fileRoutes, { prefix: "/api/v1" });
    await fastify.register(operationRoutes, { prefix: "/api/v1" });

    // Prune completed/failed operations every 60 seconds
    const pruneInterval = setInterval(() => {
        tracker.prune();
    }, 60_000);
    fastify.addHook("onClose", () => {
        clearInterval(pruneInterval);
    });

    return fastify;
}

// Augment Fastify types with decorations
declare module "fastify" {
    interface FastifyInstance {
        registry: ProviderRegistry;
        tracker: OperationTracker;
    }
}
