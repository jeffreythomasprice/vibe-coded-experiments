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
}

export class HttpClient {
  private cache?: FileCache;
  private logger?: Logger;
  private userAgent: string;
  private maxConcurrent: number;
  private minDelayMs: number;
  private maxRetries: number;
  private hosts = new Map<string, HostState>();

  constructor(options: HttpClientOptions = {}) {
    this.cache = options.cache;
    this.logger = options.logger;
    this.userAgent = options.userAgent ?? "fanfic-scraper";
    this.maxConcurrent = options.maxConcurrentPerHost ?? 2;
    this.minDelayMs = options.minDelayMs ?? 1000;
    this.maxRetries = options.maxRetries ?? 3;
  }

  private getHostState(host: string): HostState {
    let state = this.hosts.get(host);
    if (!state) {
      state = { inflight: 0, queue: [], lastRequest: 0 };
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

    const elapsed = Date.now() - state.lastRequest;
    if (elapsed < this.minDelayMs && state.lastRequest > 0) {
      await Bun.sleep(this.minDelayMs - elapsed);
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
      return await this.fetchWithRetry(url, useCache);
    } finally {
      this.releaseSlot(host);
    }
  }

  private async fetchWithRetry(
    url: string,
    useCache: boolean,
  ): Promise<HttpResponse> {
    let lastError: Error | null = null;

    for (let attempt = 0; attempt <= this.maxRetries; attempt++) {
      try {
        if (attempt === 0) {
          this.logger?.info("Fetching", { url });
        } else {
          this.logger?.warn("Retrying fetch", { url, attempt });
        }

        const res = await fetch(url, {
          headers: { "User-Agent": this.userAgent },
          redirect: "follow",
        });

        if (res.status === 429 || (res.status >= 500 && res.status < 600)) {
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

        const body = new Uint8Array(await res.arrayBuffer());
        const headers: Record<string, string> = {};
        res.headers.forEach((v, k) => (headers[k] = v));

        if (useCache && this.cache && res.ok) {
          await this.cache.put(url, res.status, headers, body);
        }

        this.logger?.debug("Fetch complete", { url, status: res.status });

        return this.toResponse(
          { status: res.status, headers, body },
          false,
        );
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

  private toResponse(
    cached: CachedResponse,
    fromCache: boolean,
  ): HttpResponse {
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
