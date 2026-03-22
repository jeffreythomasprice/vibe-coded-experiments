import type { ScraperConfig } from "./config";
import { parseDuration } from "./config";
import { FileCache } from "./cache";
import { HttpClient } from "./http";
import type { Logger } from "./logger";
import type { ForumAdapter } from "./forums/base";
import type { StoryAdapter } from "./stories/base";
import { SpaceBattlesAdapter } from "./forums/spacebattles";
import type { Subforum, Thread, Post, SiteType } from "./models";

export interface TargetInfo {
  name: string;
  type: SiteType;
  baseUrl: string;
}

const FORUM_ADAPTERS: Record<string, ForumAdapter> = {
  spacebattles: new SpaceBattlesAdapter(),
};

const STORY_ADAPTERS: Record<string, StoryAdapter> = {};

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

export function createHttpClient(
  config: ScraperConfig,
  options?: { noCache?: boolean; logger?: Logger },
): HttpClient {
  const cache =
    options?.noCache
      ? undefined
      : new FileCache(config.cache.dir, parseDuration(config.cache.ttl));

  return new HttpClient({
    cache,
    logger: options?.logger,
    userAgent: config.http.userAgent,
    maxConcurrentPerHost: config.http.maxConcurrentPerHost,
    minDelayMs: config.http.minDelayMs,
    maxRetries: config.http.maxRetries,
  });
}

function resolveSubforums(
  allSubforums: Subforum[],
  filter?: string[],
  logger?: Logger,
): Subforum[] {
  if (!filter || filter.length === 0) return allSubforums;

  return filter.flatMap((nameOrId) => {
    const match = allSubforums.find(
      (sf) =>
        sf.subforumId === nameOrId ||
        sf.name.toLowerCase() === nameOrId.toLowerCase(),
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

      const { threads, hasNext } = await adapter.getThreadList(
        http,
        subforum,
        page,
      );

      let pagePastCutoff = false;

      for (const thread of threads) {
        // Skip non-sticky threads older than cutoff (don't fetch their posts)
        if (!thread.isSticky && subforumCutoff && thread.lastUpdated && thread.lastUpdated < subforumCutoff) {
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
