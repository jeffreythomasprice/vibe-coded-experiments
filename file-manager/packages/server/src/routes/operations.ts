import type { FastifyPluginAsync } from "fastify";
import { operationSchema } from "@file-manager/shared";
import type { Operation } from "@file-manager/shared";
import type { OperationEventType } from "../operation-tracker.js";

export const operationRoutes: FastifyPluginAsync = async (fastify) => {
    // GET /api/v1/operations — list all operations, optional ?status= filter
    fastify.get<{ Querystring: { status?: string } }>("/operations", {
        schema: {
            response: {
                200: { type: "array", items: operationSchema },
            },
        },
    }, async (req) => {
        const { status } = req.query;
        if (status && !["pending", "in_progress", "completed", "failed"].includes(status)) {
            throw fastify.httpErrors.badRequest(`Invalid status filter: ${status}`);
        }
        return fastify.tracker.list(status as Operation["status"] | undefined);
    });

    // GET /api/v1/operations/events — SSE stream
    // Must be registered BEFORE the :id route to avoid matching "events" as an id
    fastify.get("/operations/events", async (req, reply) => {
        const raw = reply.raw;
        raw.writeHead(200, {
            "Content-Type": "text/event-stream",
            "Cache-Control": "no-cache",
            Connection: "keep-alive",
        });
        raw.write("\n"); // initial flush

        const eventTypes: OperationEventType[] = ["created", "progress", "completed", "failed"];

        const handler = (eventType: string) => (op: Operation) => {
            raw.write(`event: ${eventType}\ndata: ${JSON.stringify(op)}\n\n`);
        };

        const handlers = eventTypes.map((et) => {
            const h = handler(et);
            fastify.tracker.on(et, h);
            return { event: et, handler: h };
        });

        const cleanup = () => {
            for (const { event, handler: h } of handlers) {
                fastify.tracker.removeListener(event, h);
            }
        };

        req.raw.on("close", cleanup);

        // Keep the connection open — Fastify won't send a response since we used reply.raw
        await reply.hijack();
    });

    // GET /api/v1/operations/:id — single operation
    fastify.get<{ Params: { id: string } }>("/operations/:id", {
        schema: {
            response: {
                200: operationSchema,
            },
        },
    }, async (req, reply) => {
        const op = fastify.tracker.get(req.params.id);
        if (!op) {
            return reply.notFound(`Operation '${req.params.id}' not found`);
        }
        return op;
    });
};
