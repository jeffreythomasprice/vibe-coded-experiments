import type { ServerConfig } from "../src/config.js";

export default {
    database: {
        url: "postgres://filemanager:filemanager@localhost:5432/filemanager",
    },
} satisfies ServerConfig;
