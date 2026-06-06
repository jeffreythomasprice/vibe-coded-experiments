import { existsSync, readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { parse as parseTOML, stringify as stringifyTOML } from "smol-toml";

export type ListKind = "favorites" | "ignored";

export interface ListEntry {
    url: string;
    site: string;
    added_at: string;
}

export interface ListsData {
    favorites: ListEntry[];
    ignored: ListEntry[];
}

export function normalizeUrl(url: string): string {
    try {
        const parsed = new URL(url);
        const pathname = parsed.pathname.replace(/\/+$/, "");
        return `${parsed.protocol}//${parsed.hostname.toLowerCase()}${pathname}`;
    } catch {
        return url;
    }
}

export function loadLists(filePath: string): ListsData {
    if (!existsSync(filePath)) {
        return { favorites: [], ignored: [] };
    }

    const raw = readFileSync(filePath, "utf-8");
    const parsed = parseTOML(raw) as Record<string, unknown>;

    const favorites = (parsed.favorites ?? []) as ListEntry[];
    const ignored = (parsed.ignored ?? []) as ListEntry[];

    return { favorites, ignored };
}

export function saveLists(filePath: string, data: ListsData): void {
    const dir = dirname(filePath);
    if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
    }

    const toml = stringifyTOML(data);
    writeFileSync(filePath, toml, "utf-8");
}

export function buildStatusMap(data: ListsData): Map<string, ListKind> {
    const map = new Map<string, ListKind>();
    for (const entry of data.favorites) {
        map.set(normalizeUrl(entry.url), "favorites");
    }
    for (const entry of data.ignored) {
        map.set(normalizeUrl(entry.url), "ignored");
    }
    return map;
}

export function addToList(
    data: ListsData,
    kind: ListKind,
    url: string,
    site: string,
): { ok: boolean; warning?: string } {
    const normalized = normalizeUrl(url);
    const otherKind: ListKind = kind === "favorites" ? "ignored" : "favorites";
    const otherList = data[otherKind];
    const thisList = data[kind];

    if (otherList.some((e) => normalizeUrl(e.url) === normalized)) {
        return {
            ok: false,
            warning: `URL is already in the ${otherKind} list. Remove it first.`,
        };
    }

    if (thisList.some((e) => normalizeUrl(e.url) === normalized)) {
        return { ok: false, warning: `URL is already in the ${kind} list.` };
    }

    thisList.push({
        url,
        site,
        added_at: new Date().toISOString(),
    });

    return { ok: true };
}

export function removeFromList(data: ListsData, kind: ListKind, url: string): boolean {
    const normalized = normalizeUrl(url);
    const list = data[kind];
    const idx = list.findIndex((e) => normalizeUrl(e.url) === normalized);
    if (idx === -1) return false;
    list.splice(idx, 1);
    return true;
}

export function getListPrefix(status: ListKind | undefined): string {
    switch (status) {
        case "favorites":
            return "[FAV]    ";
        case "ignored":
            return "[IGNORE] ";
        default:
            return "";
    }
}
