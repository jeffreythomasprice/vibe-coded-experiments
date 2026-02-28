import type { StorageProvider, ProviderScheme } from "@file-manager/shared";
import { assertLocalProviderConfig } from "@file-manager/shared";
import { LocalProvider } from "./local.js";

export function createProvider(scheme: ProviderScheme, config: Record<string, string>): StorageProvider {
    if (scheme === "local") {
        assertLocalProviderConfig(config);
        return new LocalProvider(config.rootDir);
    }
    throw new Error(`Unsupported scheme: ${scheme}`);
}
