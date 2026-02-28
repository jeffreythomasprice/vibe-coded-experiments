import { pgTable, text, jsonb, timestamp } from "drizzle-orm/pg-core";
import type { ProviderScheme } from "@file-manager/shared";

export const providerMounts = pgTable("provider_mounts", {
    mountId: text("mount_id").primaryKey(),
    scheme: text("scheme").notNull().$type<ProviderScheme>(),
    config: jsonb("config").notNull().$type<Record<string, string>>(),
    createdAt: timestamp("created_at").defaultNow().notNull(),
});

export type ProviderMountRow = typeof providerMounts.$inferSelect;
