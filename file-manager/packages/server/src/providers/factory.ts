import type { StorageProvider, ProviderScheme } from "@file-manager/shared";
import { assertLocalProviderConfig, assertS3ProviderConfig } from "@file-manager/shared";
import { LocalProvider } from "./local.js";
import { S3Provider } from "./s3.js";

export function createProvider(scheme: ProviderScheme, config: Record<string, string>): StorageProvider {
    if (scheme === "local") {
        assertLocalProviderConfig(config);
        return new LocalProvider(config.rootDir);
    }
    if (scheme === "s3") {
        assertS3ProviderConfig(config);
        return new S3Provider(config.bucket, {
            prefix: config.prefix,
            region: config.region,
            accessKeyId: config.accessKeyId,
            secretAccessKey: config.secretAccessKey,
        });
    }
    throw new Error(`Unsupported scheme: ${scheme}`);
}
