export interface DatabaseConfig {
    url: string;
}

export interface ServerConfig {
    database: DatabaseConfig;
}

const env = process.env["NODE_ENV"] ?? "development";

export async function loadConfig(): Promise<ServerConfig> {
    switch (env) {
        case "development": {
            const mod = await import("../../config/development.js");
            return mod.default;
        }
        case "production": {
            const mod = await import("../../config/production.js");
            return mod.default;
        }
        default:
            throw new Error(`No config file for environment: ${env}`);
    }
}
