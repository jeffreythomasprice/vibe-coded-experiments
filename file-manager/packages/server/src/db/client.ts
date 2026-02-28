import postgres from "postgres";
import { drizzle } from "drizzle-orm/postgres-js";
import * as schema from "./schema.js";

export type Db = ReturnType<typeof createDb>;

export function createDb(connectionString: string) {
    const sql = postgres(connectionString, { max: 10 });
    return drizzle(sql, { schema });
}
