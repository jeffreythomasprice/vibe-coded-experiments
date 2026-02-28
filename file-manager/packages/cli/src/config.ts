import { existsSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import { assertCliConfig } from "@file-manager/schemas";
import type { CliConfig } from "@file-manager/schemas";

function getConfigDir(): string {
    const xdg = process.env["XDG_CONFIG_HOME"];
    return join(xdg ?? join(homedir(), ".config"), "file-manager");
}

export function getConfigPath(): string {
    return join(getConfigDir(), "config.json");
}

export function loadConfig(): CliConfig {
    const path = getConfigPath();
    if (!existsSync(path)) return {};
    try {
        const raw = readFileSync(path, "utf-8");
        const data: unknown = JSON.parse(raw);
        assertCliConfig(data);
        return data;
    } catch {
        return {};
    }
}

export function saveConfig(config: CliConfig): void {
    const dir = getConfigDir();
    mkdirSync(dir, { recursive: true });
    writeFileSync(getConfigPath(), JSON.stringify(config, null, 2) + "\n", { mode: 0o600 });
}
