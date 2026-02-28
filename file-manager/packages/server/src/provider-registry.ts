import { eq } from "drizzle-orm";
import type { ProviderMount, StorageProvider, ProviderScheme } from "@file-manager/shared";
import type { Db } from "./db/client.js";
import { providerMounts } from "./db/schema.js";
import { createProvider } from "./providers/factory.js";

export class ProviderRegistry {
    private db: Db | undefined;
    private mounts = new Map<string, ProviderMount>();

    async initialize(db: Db): Promise<void> {
        this.db = db;
        const rows = await db.select().from(providerMounts);
        for (const row of rows) {
            const provider = createProvider(row.scheme, row.config);
            this.mounts.set(row.mountId, { mountId: row.mountId, scheme: row.scheme, config: row.config, provider });
        }
    }

    async mount(mountId: string, scheme: ProviderScheme, config: Record<string, string>, provider: StorageProvider): Promise<void> {
        if (this.mounts.has(mountId)) {
            throw new Error(`Mount ID '${mountId}' is already in use`);
        }
        if (this.db !== undefined) {
            await this.db.insert(providerMounts).values({ mountId, scheme, config });
        }
        this.mounts.set(mountId, { mountId, scheme, config, provider });
    }

    async unmount(mountId: string): Promise<boolean> {
        if (!this.mounts.has(mountId)) return false;
        if (this.db !== undefined) {
            await this.db.delete(providerMounts).where(eq(providerMounts.mountId, mountId));
        }
        return this.mounts.delete(mountId);
    }

    get(mountId: string): ProviderMount | undefined {
        return this.mounts.get(mountId);
    }

    list(): ProviderMount[] {
        return Array.from(this.mounts.values());
    }
}
