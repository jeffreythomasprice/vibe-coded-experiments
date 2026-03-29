import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { parse as parseTOML, stringify as stringifyTOML } from "smol-toml";

export interface LoggingConfig {
    dir: string;
}

export interface CacheConfig {
    dir: string;
    ttl: string;
}

export interface HttpConfig {
    userAgent: string;
    maxConcurrentPerHost: number;
    minDelayMs: number;
    maxRetries: number;
}

export interface SubforumsConfig {
    maxPages: number;
    updatedWithin?: string;
}

export interface ThreadsConfig {
    maxPages: number;
}

export interface StoriesConfig {
    maxPages: number;
    updatedWithin?: string;
}

export interface ListsConfig {
    file: string;
}

export interface SearchSourceConfig {
    site: string;
    query: string;
}

export interface SearchConfig {
    updatedWithin: string;
    maxPages: number;
    sources: SearchSourceConfig[];
}

export interface ScraperConfig {
    logging: LoggingConfig;
    cache: CacheConfig;
    http: HttpConfig;
    subforums: SubforumsConfig;
    threads: ThreadsConfig;
    stories: StoriesConfig;
    lists: ListsConfig;
    search: SearchConfig;
}

const DEFAULTS: ScraperConfig = {
    logging: {
        dir: "/tmp/fanfic-scraper/logs",
    },
    cache: {
        dir: "/tmp/fanfic-scraper/cache",
        ttl: "1h",
    },
    http: {
        userAgent: "",
        maxConcurrentPerHost: 2,
        minDelayMs: 2000,
        maxRetries: 3,
    },
    subforums: {
        maxPages: 100,
        updatedWithin: "7d",
    },
    threads: {
        maxPages: 1000,
    },
    stories: {
        maxPages: 50,
        updatedWithin: "7d",
    },
    lists: {
        file: "/tmp/fanfic-scraper/lists.toml",
    },
    search: {
        updatedWithin: "7d",
        maxPages: 1000,
        sources: [],
    },
};

export function parseDuration(s: string): number {
    const match = s.match(/^(\d+)\s*(s|m|h|d|w)$/);
    if (!match) throw new Error(`Invalid duration: ${s}`);
    const value = parseInt(match[1]!, 10);
    const unit = match[2]!;
    const multipliers: Record<string, number> = {
        s: 1000,
        m: 60 * 1000,
        h: 60 * 60 * 1000,
        d: 24 * 60 * 60 * 1000,
        w: 7 * 24 * 60 * 60 * 1000,
    };
    return value * multipliers[unit]!;
}

function findConfigFile(explicitPath?: string): string | null {
    if (explicitPath) {
        if (existsSync(explicitPath)) return explicitPath;
        throw new Error(`Config file not found: ${explicitPath}`);
    }

    const candidates = [
        join(process.cwd(), "config.toml"),
        join(homedir(), ".config", "fanfic-scraper", "config.toml"),
    ];

    for (const path of candidates) {
        if (existsSync(path)) return path;
    }

    return null;
}

export function configToToml(config: ScraperConfig): string {
    const obj: Record<string, Record<string, unknown>> = {
        logging: { dir: config.logging.dir },
        cache: { dir: config.cache.dir, ttl: config.cache.ttl },
        http: {
            user_agent: config.http.userAgent,
            max_concurrent_per_host: config.http.maxConcurrentPerHost,
            min_delay_ms: config.http.minDelayMs,
            max_retries: config.http.maxRetries,
        },
        subforums: {
            max_pages: config.subforums.maxPages,
            ...(config.subforums.updatedWithin && {
                updated_within: config.subforums.updatedWithin,
            }),
        },
        threads: { max_pages: config.threads.maxPages },
        stories: {
            max_pages: config.stories.maxPages,
            ...(config.stories.updatedWithin && { updated_within: config.stories.updatedWithin }),
        },
        lists: { file: config.lists.file },
        search: {
            updated_within: config.search.updatedWithin,
            max_pages: config.search.maxPages,
            sources: config.search.sources.map((s) => ({
                site: s.site,
                query: s.query,
            })),
        },
    };
    return stringifyTOML(obj);
}

export function loadConfig(explicitPath?: string): ScraperConfig {
    const configPath = findConfigFile(explicitPath);

    if (!configPath) {
        return { ...DEFAULTS };
    }

    const raw = readFileSync(configPath, "utf-8");
    const parsed = parseTOML(raw) as Record<string, unknown>;

    const loggingRaw = (parsed.logging ?? {}) as Record<string, unknown>;
    const cacheRaw = (parsed.cache ?? {}) as Record<string, unknown>;
    const httpRaw = (parsed.http ?? {}) as Record<string, unknown>;
    const subforumsRaw = (parsed.subforums ?? {}) as Record<string, unknown>;
    const threadsRaw = (parsed.threads ?? {}) as Record<string, unknown>;
    const storiesRaw = (parsed.stories ?? {}) as Record<string, unknown>;
    const listsRaw = (parsed.lists ?? {}) as Record<string, unknown>;
    const searchRaw = (parsed.search ?? {}) as Record<string, unknown>;
    const searchSourcesRaw = (Array.isArray(searchRaw.sources) ? searchRaw.sources : []) as Array<
        Record<string, unknown>
    >;

    return {
        logging: {
            dir: (loggingRaw.dir as string) ?? DEFAULTS.logging.dir,
        },
        cache: {
            dir: (cacheRaw.dir as string) ?? DEFAULTS.cache.dir,
            ttl: (cacheRaw.ttl as string) ?? DEFAULTS.cache.ttl,
        },
        http: {
            userAgent: (httpRaw.user_agent as string) ?? DEFAULTS.http.userAgent,
            maxConcurrentPerHost:
                (httpRaw.max_concurrent_per_host as number) ?? DEFAULTS.http.maxConcurrentPerHost,
            minDelayMs: (httpRaw.min_delay_ms as number) ?? DEFAULTS.http.minDelayMs,
            maxRetries: (httpRaw.max_retries as number) ?? DEFAULTS.http.maxRetries,
        },
        subforums: {
            maxPages: (subforumsRaw.max_pages as number) ?? DEFAULTS.subforums.maxPages,
            updatedWithin:
                (subforumsRaw.updated_within as string) ?? DEFAULTS.subforums.updatedWithin,
        },
        threads: {
            maxPages: (threadsRaw.max_pages as number) ?? DEFAULTS.threads.maxPages,
        },
        stories: {
            maxPages: (storiesRaw.max_pages as number) ?? DEFAULTS.stories.maxPages,
            updatedWithin: (storiesRaw.updated_within as string) ?? DEFAULTS.stories.updatedWithin,
        },
        lists: {
            file: (listsRaw.file as string) ?? DEFAULTS.lists.file,
        },
        search: {
            updatedWithin: (searchRaw.updated_within as string) ?? DEFAULTS.search.updatedWithin,
            maxPages: (searchRaw.max_pages as number) ?? DEFAULTS.search.maxPages,
            sources: searchSourcesRaw.map((s) => ({
                site: s.site as string,
                query: s.query as string,
            })),
        },
    };
}
