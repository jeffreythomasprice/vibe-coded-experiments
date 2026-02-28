import { Command } from "commander";
import { getConfigPath, loadConfig, saveConfig } from "../config.js";

const KNOWN_KEYS = ["apiUrl"] as const;
type KnownKey = (typeof KNOWN_KEYS)[number];

function isKnownKey(key: string): key is KnownKey {
    return (KNOWN_KEYS as readonly string[]).includes(key);
}

export function registerConfigCommands(program: Command): void {
    const config = program.command("config").description("Manage CLI configuration");

    config
        .command("show")
        .description("Show the current configuration and where each value comes from")
        .action(() => {
            const cfg = loadConfig();
            console.log(`Config file: ${getConfigPath()}\n`);

            const envUrl = process.env["FILE_MANAGER_API_URL"];
            let apiUrlSource: string;
            let apiUrlValue: string;
            if (envUrl !== undefined) {
                apiUrlSource = "[env]";
                apiUrlValue = envUrl;
            } else if (cfg.apiUrl !== undefined) {
                apiUrlSource = "[file]";
                apiUrlValue = cfg.apiUrl;
            } else {
                apiUrlSource = "[default]";
                apiUrlValue = "http://localhost:8000";
            }

            console.log(`  apiUrl  ${apiUrlValue}  ${apiUrlSource}`);
        });

    config
        .command("set <key> <value>")
        .description(`Set a config value. Known keys: ${KNOWN_KEYS.join(", ")}`)
        .action((key: string, value: string) => {
            if (!isKnownKey(key)) {
                console.error(`Unknown config key: ${key}. Known keys: ${KNOWN_KEYS.join(", ")}`);
                process.exit(1);
            }
            const cfg = loadConfig();
            cfg[key] = value;
            saveConfig(cfg);
            console.log(`Set ${key} = ${value}`);
        });

    config
        .command("unset <key>")
        .description("Remove a config value (revert to default)")
        .action((key: string) => {
            if (!isKnownKey(key)) {
                console.error(`Unknown config key: ${key}. Known keys: ${KNOWN_KEYS.join(", ")}`);
                process.exit(1);
            }
            const cfg = loadConfig();
            delete cfg[key];
            saveConfig(cfg);
            console.log(`Unset ${key}`);
        });
}
