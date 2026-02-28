import type { Config } from "drizzle-kit";
import { loadConfig } from "./src/config.js";

const config = await loadConfig();

export default {
    schema: "./src/db/schema.ts",
    out: "./drizzle",
    dialect: "postgresql",
    dbCredentials: {
        url: config.database.url,
    },
} satisfies Config;
