import type { FileCache, CachedResponse } from "./cache";
import type { Logger } from "./logger";

export interface HttpClientOptions {
    cache?: FileCache;
    logger?: Logger;
    userAgent?: string;
    maxConcurrentPerHost?: number;
    minDelayMs?: number;
    maxRetries?: number;
}

export interface HttpResponse {
    status: number;
    headers: Record<string, string>;
    body: Uint8Array;
    text(): string;
    cached: boolean;
}

interface HostState {
    inflight: number;
    queue: Array<() => void>;
    lastRequest: number;
    /** Host-wide cooldown (epoch ms) set after a 429; all requests wait until it passes. */
    blockedUntil: number;
    /** Extra per-request delay added after rate limiting; decays on success. */
    penaltyMs: number;
    /** Consecutive rate-limit episodes for this host; reset by any non-429 response. */
    rateLimitStreak: number;
    /** When the current cooldown was triggered; 429s from requests sent earlier don't re-escalate. */
    cooldownStartedAt: number;
}

const RATE_LIMIT_BASE_MS = 5000;
const RATE_LIMIT_MAX_MS = 120000;
const MAX_PENALTY_MS = 10000;
const MAX_RATE_LIMIT_RETRIES = 6;

function parseRetryAfter(value: string | null): number | null {
    if (!value) return null;
    const secs = Number(value);
    if (Number.isFinite(secs)) return Math.max(0, secs * 1000);
    const date = Date.parse(value);
    return Number.isNaN(date) ? null : Math.max(0, date - Date.now());
}

const DEFAULT_USER_AGENT =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

const BROWSER_HEADERS: Record<string, string> = {
    Accept: "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8",
    "Accept-Language": "en-US,en;q=0.9",
    "Accept-Encoding": "gzip, deflate, br",
    "Sec-CH-UA": '"Not_A Brand";v="8", "Chromium";v="120", "Google Chrome";v="120"',
    "Sec-CH-UA-Mobile": "?0",
    "Sec-CH-UA-Platform": '"Windows"',
    "Sec-Fetch-Dest": "document",
    "Sec-Fetch-Mode": "navigate",
    "Sec-Fetch-Site": "none",
    "Sec-Fetch-User": "?1",
    "Upgrade-Insecure-Requests": "1",
};

export class HttpClient {
    private cache?: FileCache;
    private logger?: Logger;
    private userAgent: string;
    private maxConcurrent: number;
    private minDelayMs: number;
    private maxRetries: number;
    private hosts = new Map<string, HostState>();
    private hostMinDelays = new Map<string, number>();

    constructor(options: HttpClientOptions = {}) {
        this.cache = options.cache;
        this.logger = options.logger;
        this.userAgent = options.userAgent || DEFAULT_USER_AGENT;
        this.maxConcurrent = options.maxConcurrentPerHost ?? 2;
        this.minDelayMs = options.minDelayMs ?? 1000;
        this.maxRetries = options.maxRetries ?? 3;
    }

    setHostMinDelay(host: string, delayMs: number): void {
        this.hostMinDelays.set(host, delayMs);
    }

    private getHostState(host: string): HostState {
        let state = this.hosts.get(host);
        if (!state) {
            state = {
                inflight: 0,
                queue: [],
                lastRequest: 0,
                blockedUntil: 0,
                penaltyMs: 0,
                rateLimitStreak: 0,
                cooldownStartedAt: 0,
            };
            this.hosts.set(host, state);
        }
        return state;
    }

    private async acquireSlot(host: string): Promise<void> {
        const state = this.getHostState(host);

        if (state.inflight >= this.maxConcurrent) {
            await new Promise<void>((resolve) => state.queue.push(resolve));
        }

        state.inflight++;

        await this.waitForCooldown(state);

        const minDelay = (this.hostMinDelays.get(host) ?? this.minDelayMs) + state.penaltyMs;
        const elapsed = Date.now() - state.lastRequest;
        if (elapsed < minDelay && state.lastRequest > 0) {
            await Bun.sleep(minDelay - elapsed);
        }
    }

    /** Wait out any host-wide cooldown (re-checks, since it can be extended while waiting). */
    private async waitForCooldown(state: HostState): Promise<void> {
        while (state.blockedUntil > Date.now()) {
            await Bun.sleep(state.blockedUntil - Date.now());
        }
    }

    private releaseSlot(host: string): void {
        const state = this.getHostState(host);
        state.inflight--;
        state.lastRequest = Date.now();

        const next = state.queue.shift();
        if (next) next();
    }

    async get(
        url: string,
        options?: { useCache?: boolean; cacheTtlMs?: number },
    ): Promise<HttpResponse> {
        const useCache = options?.useCache ?? true;

        if (useCache && this.cache) {
            const cached = await this.cache.get(url, options?.cacheTtlMs);
            if (cached) {
                this.logger?.debug("Cache hit", { url });
                return this.toResponse(cached, true);
            }
            this.logger?.debug("Cache miss", { url });
        }

        const host = new URL(url).host;
        await this.acquireSlot(host);

        try {
            return await this.fetchWithRetry(url, useCache, host);
        } finally {
            this.releaseSlot(host);
        }
    }

    private async fetchWithRetry(
        url: string,
        useCache: boolean,
        host: string,
    ): Promise<HttpResponse> {
        let lastError: Error | null = null;
        const state = this.getHostState(host);
        let rateLimitHits = 0;

        for (let attempt = 0; attempt <= this.maxRetries + rateLimitHits; attempt++) {
            try {
                // Retries of every in-flight request share the host cooldown.
                await this.waitForCooldown(state);
                const sentAt = Date.now();

                if (attempt === 0) {
                    this.logger?.info("Fetching", { url });
                } else {
                    this.logger?.warn("Retrying fetch", { url, attempt });
                }

                const res = await fetch(url, {
                    headers: {
                        "User-Agent": this.userAgent,
                        ...BROWSER_HEADERS,
                    },
                    redirect: "follow",
                });

                if (res.status === 429) {
                    // Rate limited: pause the whole host so every in-flight request backs
                    // off together. Only the first 429 of a burst (i.e. from a request sent
                    // after the current cooldown began) escalates the backoff; the rest just
                    // wait out the shared cooldown.
                    rateLimitHits++;
                    await res.arrayBuffer().catch(() => {});
                    if (rateLimitHits <= MAX_RATE_LIMIT_RETRIES) {
                        if (sentAt >= state.cooldownStartedAt) {
                            state.rateLimitStreak++;
                            state.penaltyMs = Math.min(
                                Math.max(state.penaltyMs * 2, 1000),
                                MAX_PENALTY_MS,
                            );
                            const retryAfter = parseRetryAfter(res.headers.get("retry-after"));
                            const backoff = Math.min(
                                RATE_LIMIT_BASE_MS * 2 ** (state.rateLimitStreak - 1),
                                RATE_LIMIT_MAX_MS,
                            );
                            const delay = Math.min(
                                Math.max(retryAfter ?? 0, backoff) + Math.random() * 1000,
                                RATE_LIMIT_MAX_MS * 2,
                            );
                            state.cooldownStartedAt = Date.now();
                            state.blockedUntil = Math.max(state.blockedUntil, Date.now() + delay);
                            this.logger?.warn("Rate limited (429), cooling down host", {
                                url,
                                retryAfter: res.headers.get("retry-after"),
                                retryInMs: Math.round(delay),
                                streak: state.rateLimitStreak,
                                penaltyMs: state.penaltyMs,
                            });
                        } else {
                            this.logger?.debug("Rate limited (429), joining existing cooldown", {
                                url,
                            });
                        }
                        continue; // waitForCooldown at loop start handles the sleep
                    }
                } else if (res.status >= 500 && res.status < 600) {
                    if (attempt < this.maxRetries) {
                        const delay = Math.min(1000 * 2 ** attempt, 30000);
                        this.logger?.warn("Retryable HTTP error", {
                            url,
                            status: res.status,
                            retryInMs: delay,
                        });
                        await Bun.sleep(delay);
                        continue;
                    }
                }

                if (res.status !== 429) {
                    state.rateLimitStreak = 0;
                    state.penaltyMs = Math.floor(state.penaltyMs / 2);
                }

                const body = new Uint8Array(await res.arrayBuffer());
                const headers: Record<string, string> = {};
                res.headers.forEach((v, k) => (headers[k] = v));

                if (useCache && this.cache && res.ok) {
                    await this.cache.put(url, res.status, headers, body);
                }

                this.logger?.debug("Fetch complete", { url, status: res.status });

                return this.toResponse({ status: res.status, headers, body }, false);
            } catch (err) {
                lastError = err as Error;
                if (attempt < this.maxRetries) {
                    const delay = Math.min(1000 * 2 ** attempt, 30000);
                    this.logger?.error("Fetch error, retrying", {
                        url,
                        error: lastError.message,
                        retryInMs: delay,
                    });
                    await Bun.sleep(delay);
                }
            }
        }

        throw lastError ?? new Error(`Failed to fetch ${url}`);
    }

    private toResponse(cached: CachedResponse, fromCache: boolean): HttpResponse {
        const decoder = new TextDecoder();
        return {
            status: cached.status,
            headers: cached.headers,
            body: cached.body,
            text: () => decoder.decode(cached.body),
            cached: fromCache,
        };
    }
}
