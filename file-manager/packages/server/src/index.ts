import { runMigrations } from "./db/migrate.js";
import { createDb } from "./db/client.js";
import { createServer } from "./server.js";

const port = parseInt(process.env["PORT"] ?? "8000", 10);
const host = process.env["HOST"] ?? "0.0.0.0";

const databaseUrl = process.env["DATABASE_URL"];
if (!databaseUrl) {
    console.error("DATABASE_URL required");
    process.exit(1);
}

await runMigrations(databaseUrl);
const db = createDb(databaseUrl);
const server = await createServer({ db });

try {
    await server.listen({ port, host });
    console.log(`Server listening on http://${host}:${port}`);
} catch (err) {
    server.log.error(err);
    process.exit(1);
}
