import { drizzle } from "drizzle-orm/postgres-js";
import { migrate } from "drizzle-orm/postgres-js/migrator";
import postgres from "postgres";

export async function runMigrations(connectionString: string): Promise<void> {
    const sql = postgres(connectionString, { max: 1 });
    await migrate(drizzle(sql), { migrationsFolder: "drizzle" });
    await sql.end();
}

if (import.meta.main) {
    const { loadConfig } = await import("../config.js");
    const config = await loadConfig();
    await runMigrations(config.database.url);
}
