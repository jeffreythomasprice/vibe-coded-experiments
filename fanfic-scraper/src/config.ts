import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { parse as parseTOML } from "smol-toml";

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
  updatedWithin?: string;
}

export interface ScraperConfig {
  logging: LoggingConfig;
  cache: CacheConfig;
  http: HttpConfig;
  subforums: SubforumsConfig;
  threads: ThreadsConfig;
  stories: StoriesConfig;
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
    userAgent: "fanfic-scraper",
    maxConcurrentPerHost: 2,
    minDelayMs: 1000,
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
    updatedWithin: "7d",
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
        (httpRaw.max_concurrent_per_host as number) ??
        DEFAULTS.http.maxConcurrentPerHost,
      minDelayMs:
        (httpRaw.min_delay_ms as number) ?? DEFAULTS.http.minDelayMs,
      maxRetries:
        (httpRaw.max_retries as number) ?? DEFAULTS.http.maxRetries,
    },
    subforums: {
      maxPages:
        (subforumsRaw.max_pages as number) ?? DEFAULTS.subforums.maxPages,
      updatedWithin:
        (subforumsRaw.updated_within as string) ?? DEFAULTS.subforums.updatedWithin,
    },
    threads: {
      maxPages:
        (threadsRaw.max_pages as number) ?? DEFAULTS.threads.maxPages,
    },
    stories: {
      updatedWithin:
        (storiesRaw.updated_within as string) ?? DEFAULTS.stories.updatedWithin,
    },
  };
}
