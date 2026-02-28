import Fastify from "fastify";
import sensible from "@fastify/sensible";
import { ProviderRegistry } from "./provider-registry.js";
import { providerRoutes } from "./routes/providers.js";
import { fileRoutes } from "./routes/files.js";

export async function createServer() {
    const fastify = Fastify({
        logger: true,
    });

    await fastify.register(sensible);

    // Accept raw binary uploads
    fastify.addContentTypeParser("application/octet-stream", { parseAs: "buffer" }, (_req, body, done) => {
        done(null, body as Buffer);
    });

    const registry = new ProviderRegistry();

    // Make registry available to routes via decoration
    fastify.decorate("registry", registry);

    await fastify.register(providerRoutes, { prefix: "/api/v1" });
    await fastify.register(fileRoutes, { prefix: "/api/v1" });

    return fastify;
}

// Augment Fastify types with registry decoration
declare module "fastify" {
    interface FastifyInstance {
        registry: ProviderRegistry;
    }
}
