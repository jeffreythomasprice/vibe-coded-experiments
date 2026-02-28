import type { ProviderMount, StorageProvider, ProviderScheme } from "@file-manager/shared";

export class ProviderRegistry {
    private mounts = new Map<string, ProviderMount>();

    mount(mountId: string, scheme: ProviderScheme, config: Record<string, string>, provider: StorageProvider): void {
        if (this.mounts.has(mountId)) {
            throw new Error(`Mount ID '${mountId}' is already in use`);
        }
        this.mounts.set(mountId, { mountId, scheme, config, provider });
    }

    unmount(mountId: string): boolean {
        return this.mounts.delete(mountId);
    }

    get(mountId: string): ProviderMount | undefined {
        return this.mounts.get(mountId);
    }

    list(): ProviderMount[] {
        return Array.from(this.mounts.values());
    }
}
