import { runMigrations } from "./db/migrate.js";
import { createDb } from "./db/client.js";
import { createServer } from "./server.js";
import { loadConfig } from "./config.js";

const port = parseInt(process.env["PORT"] ?? "8000", 10);
const host = process.env["HOST"] ?? "0.0.0.0";

const config = await loadConfig();

await runMigrations(config.database.url);
const db = createDb(config.database.url);
const server = await createServer({ db });

try {
    await server.listen({ port, host });
    console.log(`Server listening on http://${host}:${port}`);
} catch (err) {
    server.log.error(err);
    process.exit(1);
}
