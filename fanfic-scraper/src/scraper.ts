import type { ScraperConfig } from "./config";
import { parseDuration } from "./config";
import { FileCache } from "./cache";
import { HttpClient } from "./http";
import type { Logger } from "./logger";
import type { ForumAdapter } from "./forums/base";
import type { StoryAdapter } from "./stories/base";
import { SpaceBattlesAdapter } from "./forums/spacebattles";
import { SufficientVelocityAdapter } from "./forums/sufficientvelocity";
import { AO3Adapter } from "./stories/ao3";
import type { Subforum, Thread, Post, Story, SiteType } from "./models";

export interface TargetInfo {
    name: string;
    type: SiteType;
    baseUrl: string;
}

const FORUM_ADAPTERS: Record<string, ForumAdapter> = {
    spacebattles: new SpaceBattlesAdapter(),
    sufficientvelocity: new SufficientVelocityAdapter(),
};

const STORY_ADAPTERS: Record<string, StoryAdapter> = {
    ao3: new AO3Adapter(),
};

export function getSiteType(name: string): SiteType | null {
    if (name in FORUM_ADAPTERS) return "forum";
    if (name in STORY_ADAPTERS) return "story";
    return null;
}

export function detectSite(url: string): { site: string; type: SiteType } | null {
    try {
        const parsed = new URL(url);
        for (const [name, adapter] of Object.entries(FORUM_ADAPTERS)) {
            const base = new URL(adapter.baseUrl);
            if (parsed.hostname === base.hostname) {
                return { site: name, type: "forum" };
            }
        }
        for (const [name, adapter] of Object.entries(STORY_ADAPTERS)) {
            const base = new URL(adapter.baseUrl);
            if (parsed.hostname === base.hostname) {
                return { site: name, type: "story" };
            }
        }
    } catch {
        return null;
    }
    return null;
}

export function getAdapter(site: string): ForumAdapter {
    const adapter = FORUM_ADAPTERS[site];
    if (!adapter) {
        throw new Error(
            `Unknown site: ${site}. Available: ${Object.keys(FORUM_ADAPTERS).join(", ")}`,
        );
    }
    return adapter;
}

export function getTargets(): TargetInfo[] {
    const targets: TargetInfo[] = [];
    for (const [name, adapter] of Object.entries(FORUM_ADAPTERS)) {
        targets.push({ name, type: "forum", baseUrl: adapter.baseUrl });
    }
    for (const [name, adapter] of Object.entries(STORY_ADAPTERS)) {
        targets.push({ name, type: "story", baseUrl: adapter.baseUrl });
    }
    return targets;
}

export function getStoryAdapter(site: string): StoryAdapter {
    const adapter = STORY_ADAPTERS[site];
    if (!adapter) {
        throw new Error(
            `Unknown story site: ${site}. Available: ${Object.keys(STORY_ADAPTERS).join(", ")}`,
        );
    }
    return adapter;
}

export function createHttpClient(
    config: ScraperConfig,
    options?: { noCache?: boolean; logger?: Logger },
): HttpClient {
    const cache = options?.noCache
        ? undefined
        : new FileCache(config.cache.dir, parseDuration(config.cache.ttl));

    const client = new HttpClient({
        cache,
        logger: options?.logger,
        userAgent: config.http.userAgent,
        maxConcurrentPerHost: config.http.maxConcurrentPerHost,
        minDelayMs: config.http.minDelayMs,
        maxRetries: config.http.maxRetries,
    });

    // Register per-target anti-scraper delays
    for (const adapter of Object.values(FORUM_ADAPTERS)) {
        if (adapter.antiScraperDelayMs) {
            const host = new URL(adapter.baseUrl).host;
            client.setHostMinDelay(host, adapter.antiScraperDelayMs);
        }
    }
    for (const adapter of Object.values(STORY_ADAPTERS)) {
        if (adapter.antiScraperDelayMs) {
            const host = new URL(adapter.baseUrl).host;
            client.setHostMinDelay(host, adapter.antiScraperDelayMs);
        }
    }

    return client;
}

function resolveSubforums(
    allSubforums: Subforum[],
    filter?: string[],
    logger?: Logger,
): Subforum[] {
    if (!filter || filter.length === 0) return allSubforums;

    return filter.flatMap((nameOrId) => {
        const match = allSubforums.find(
            (sf) => sf.subforumId === nameOrId || sf.name.toLowerCase() === nameOrId.toLowerCase(),
        );
        if (!match) {
            logger?.warn("Subforum not found", { query: nameOrId });
            return [];
        }
        return [match];
    });
}

export interface ScrapeResult {
    subforum: Subforum;
    threads: Array<{
        thread: Thread;
        posts: Post[];
    }>;
}

export interface ScrapeOptions {
    site: string;
    subforums?: string[];
    logger?: Logger;
}

export async function scrapeForumTarget(
    http: HttpClient,
    config: ScraperConfig,
    options: ScrapeOptions,
): Promise<ScrapeResult[]> {
    const { logger } = options;
    const adapter = getAdapter(options.site);

    logger?.info("Listing subforums", { site: options.site });
    const allSubforums = await adapter.getSubforums(http);
    logger?.info("Found subforums", {
        site: options.site,
        count: allSubforums.length,
    });

    const subforums = resolveSubforums(allSubforums, options.subforums, logger);

    if (subforums.length === 0) {
        logger?.warn("No matching subforums found", { site: options.site });
        return [];
    }

    const subforumCutoff = config.subforums.updatedWithin
        ? new Date(Date.now() - parseDuration(config.subforums.updatedWithin))
        : null;

    const results: ScrapeResult[] = [];

    for (const subforum of subforums) {
        logger?.info("Scanning subforum", {
            subforum: subforum.name,
            subforumId: subforum.subforumId,
        });
        const threadResults: ScrapeResult["threads"] = [];

        let page = 1;
        let done = false;
        let threadCount = 0;

        while (page <= config.subforums.maxPages && !done) {
            logger?.debug("Fetching thread list page", {
                subforum: subforum.name,
                page,
            });

            const { threads, hasNext } = await adapter.getThreadList(http, subforum, page);

            let pagePastCutoff = false;

            for (const thread of threads) {
                // Skip non-sticky threads older than cutoff (don't fetch their posts)
                if (
                    !thread.isSticky &&
                    subforumCutoff &&
                    thread.lastUpdated &&
                    thread.lastUpdated < subforumCutoff
                ) {
                    logger?.debug("Skipping thread (older than cutoff)", {
                        thread: thread.title,
                        lastUpdated: thread.lastUpdated.toISOString(),
                        cutoff: subforumCutoff.toISOString(),
                    });
                    pagePastCutoff = true;
                    continue;
                }

                logger?.info("Fetching posts for thread", {
                    thread: thread.title,
                    threadId: thread.threadId,
                });
                const allPosts: Post[] = [];
                let postPage = 1;
                let hasMorePosts = true;

                while (hasMorePosts && postPage <= config.threads.maxPages) {
                    const { posts, hasNext: morePages } = await adapter.getPosts(
                        http,
                        thread,
                        postPage,
                    );
                    allPosts.push(...posts);
                    hasMorePosts = morePages;
                    postPage++;
                }

                logger?.info("Fetched posts for thread", {
                    thread: thread.title,
                    threadId: thread.threadId,
                    postCount: allPosts.length,
                    pages: postPage - 1,
                });

                threadResults.push({ thread, posts: allPosts });
                threadCount++;
            }

            if (pagePastCutoff) {
                logger?.debug("Stopping pagination: non-sticky thread exceeded cutoff");
                done = true;
            }
            if (!hasNext) break;
            page++;
        }

        logger?.info("Subforum scan complete", {
            subforum: subforum.name,
            threadsFound: threadCount,
            totalPosts: threadResults.reduce((sum, t) => sum + t.posts.length, 0),
        });

        results.push({ subforum, threads: threadResults });
    }

    return results;
}

export interface StoryScrapeOptions {
    site: string;
    category: string;
    logger?: Logger;
}

export async function scrapeStoryTarget(
    http: HttpClient,
    config: ScraperConfig,
    options: StoryScrapeOptions,
): Promise<Story[]> {
    const { logger } = options;
    const adapter = getStoryAdapter(options.site);

    const storyCutoff = config.stories.updatedWithin
        ? new Date(Date.now() - parseDuration(config.stories.updatedWithin))
        : null;

    const allStories: Story[] = [];

    for (let page = 1; page <= config.stories.maxPages; page++) {
        logger?.debug("Fetching story list page", {
            category: options.category,
            page,
        });

        const { stories, hasNext } = await adapter.getStoryList(http, options.category, page);

        let pagePastCutoff = false;

        for (const story of stories) {
            if (storyCutoff && story.lastUpdated && story.lastUpdated < storyCutoff) {
                logger?.debug("Skipping story (older than cutoff)", {
                    story: story.title,
                    lastUpdated: story.lastUpdated.toISOString(),
                    cutoff: storyCutoff.toISOString(),
                });
                pagePastCutoff = true;
                continue;
            }

            allStories.push(story);
        }

        if (pagePastCutoff || !hasNext) {
            if (pagePastCutoff) {
                logger?.debug("Stopping pagination: story exceeded cutoff");
            }
            break;
        }
    }

    logger?.info("Story listing complete", {
        category: options.category,
        storiesFound: allStories.length,
    });

    return allStories;
}
