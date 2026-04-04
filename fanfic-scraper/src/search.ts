import Fuse from "fuse.js";
import type { Thread, Story } from "./models";
import type { ScraperConfig, SearchSourceConfig } from "./config";
import { parseDuration } from "./config";
import { getAdapter, getStoryAdapter, getSiteType } from "./scraper";
import { buildStatusMap, getListPrefix, loadLists } from "./lists";
import type { HttpClient } from "./http";
import type { Logger } from "./logger";

export type SearchResultItem =
    | { kind: "thread"; data: Thread; contentPreview?: string; sourceIndex?: number }
    | { kind: "story"; data: Story; contentPreview?: string; sourceIndex?: number };

export interface ScoredSearchResult {
    item: SearchResultItem;
    tagScore: number;
    keywordScore: number;
    listStatus?: "favorites" | "ignored";
}

export interface SearchSource {
    site: string;
    query: string;
}

export interface SearchOptions {
    sources: SearchSource[];
    sites?: string[];
    tags?: string[];
    tagMode: "any" | "all";
    keywords?: string[];
    fetchContent: boolean;
    updatedWithin?: string;
    showIgnored: boolean;
    favoritesOnly: boolean;
    maxPages: number;
    sortMode: "mixed" | "separate";
}

export function parseSourceArg(arg: string): SearchSource {
    const colonIdx = arg.indexOf(":");
    if (colonIdx === -1) {
        throw new Error(
            `Invalid source format: "${arg}". Expected site:"query" (e.g. ao3:"Parahumans Series - Wildbow")`,
        );
    }
    const site = arg.slice(0, colonIdx);
    let query = arg.slice(colonIdx + 1);
    // Strip surrounding quotes
    if (
        (query.startsWith('"') && query.endsWith('"')) ||
        (query.startsWith("'") && query.endsWith("'"))
    ) {
        query = query.slice(1, -1);
    }
    return { site, query };
}

export function resolveSearchSources(
    cliSources: string[],
    configSources: SearchSourceConfig[],
    siteFilter?: string[],
): SearchSource[] {
    let sources: SearchSource[];

    if (cliSources.length > 0) {
        sources = cliSources.map(parseSourceArg);
    } else {
        sources = configSources.map((s) => ({ site: s.site, query: s.query }));
    }

    if (siteFilter && siteFilter.length > 0) {
        const allowed = new Set(siteFilter.map((s) => s.toLowerCase()));
        sources = sources.filter((s) => allowed.has(s.site.toLowerCase()));
    }

    return sources;
}

function getItemTags(item: SearchResultItem): string[] {
    if (item.kind === "thread") {
        const t = item.data;
        return [...t.tags, ...t.fandoms, ...t.genres, ...t.characters];
    } else {
        const s = item.data;
        return [
            ...s.tags,
            ...(s.fandoms ?? []),
            ...(s.characters ?? []),
            ...(s.relationships ?? []),
            ...s.categories,
        ];
    }
}

function getItemUrl(item: SearchResultItem): string {
    return item.data.url;
}

function getItemTitle(item: SearchResultItem): string {
    return item.data.title;
}

function getItemSearchText(item: SearchResultItem): string[] {
    const texts = [item.data.title];
    if (item.kind === "story" && item.data.summary) {
        texts.push(item.data.summary);
    }
    return texts;
}

function fuzzyMatchTags(
    itemTags: string[],
    searchTags: string[],
    mode: "any" | "all",
): { pass: boolean; score: number } {
    if (itemTags.length === 0) return { pass: false, score: 0 };

    const fuse = new Fuse(itemTags, { threshold: 0.4, includeScore: true });
    const scores: number[] = [];

    for (const tag of searchTags) {
        const results = fuse.search(tag);
        if (results.length > 0) {
            scores.push(1 - (results[0]!.score ?? 1));
        } else {
            scores.push(0);
        }
    }

    if (mode === "all") {
        const allPass = scores.every((s) => s > 0);
        return { pass: allPass, score: allPass ? Math.min(...scores) : 0 };
    } else {
        const best = Math.max(...scores);
        return { pass: best > 0, score: best };
    }
}

function fuzzyMatchKeywords(
    searchFields: string[],
    keywords: string[],
): { pass: boolean; score: number } {
    if (searchFields.length === 0) return { pass: false, score: 0 };

    const fuse = new Fuse(searchFields, { threshold: 0.4, includeScore: true });
    let bestScore = 0;

    for (const kw of keywords) {
        const results = fuse.search(kw);
        if (results.length > 0) {
            bestScore = Math.max(bestScore, 1 - (results[0]!.score ?? 1));
        }
    }

    return { pass: bestScore > 0, score: bestScore };
}

function stripHtml(html: string): string {
    return html
        .replace(/<[^>]+>/g, "")
        .replace(/\s+/g, " ")
        .trim();
}

async function collectFromSource(
    http: HttpClient,
    source: SearchSource,
    cutoff: Date | null,
    options: SearchOptions,
    logger?: Logger,
): Promise<SearchResultItem[]> {
    const siteType = getSiteType(source.site);
    if (!siteType) {
        logger?.warn("Unknown site in search source, skipping", { site: source.site });
        return [];
    }

    const items: SearchResultItem[] = [];

    if (siteType === "forum") {
        const adapter = getAdapter(source.site);
        const allSubforums = await adapter.getSubforums(http);
        const subforum = allSubforums.find(
            (sf) =>
                sf.subforumId === source.query ||
                sf.name.toLowerCase() === source.query.toLowerCase(),
        );

        if (!subforum) {
            logger?.warn("Subforum not found for search source", {
                site: source.site,
                query: source.query,
                available: allSubforums.map((sf) => sf.name),
            });
            return [];
        }

        logger?.info("Searching forum source", {
            site: source.site,
            subforum: subforum.name,
        });

        for (let page = 1; page <= options.maxPages; page++) {
            logger?.debug("Fetching thread list page", { subforum: subforum.name, page });
            const { threads, hasNext } = await adapter.getThreadList(http, subforum, page);

            let pagePastCutoff = false;

            for (const t of threads) {
                if (!t.isSticky && cutoff && t.lastUpdated && t.lastUpdated < cutoff) {
                    pagePastCutoff = true;
                    continue;
                }
                items.push({ kind: "thread", data: t });
            }

            if (pagePastCutoff || !hasNext) break;
        }
    } else {
        const adapter = getStoryAdapter(source.site);

        logger?.info("Searching story source", {
            site: source.site,
            tag: source.query,
        });

        for (let page = 1; page <= options.maxPages; page++) {
            logger?.debug("Fetching story list page", { tag: source.query, page });
            const { stories, hasNext } = await adapter.getStoryList(http, source.query, page);

            let pagePastCutoff = false;

            for (const s of stories) {
                if (cutoff && s.lastUpdated && s.lastUpdated < cutoff) {
                    pagePastCutoff = true;
                    continue;
                }
                items.push({ kind: "story", data: s });
            }

            if (pagePastCutoff || !hasNext) break;
        }
    }

    return items;
}

async function collectItems(
    http: HttpClient,
    sources: SearchSource[],
    options: SearchOptions,
    logger?: Logger,
): Promise<SearchResultItem[]> {
    const cutoff = options.updatedWithin
        ? new Date(Date.now() - parseDuration(options.updatedWithin))
        : null;

    const perSourceResults = await Promise.all(
        sources.map((source) => collectFromSource(http, source, cutoff, options, logger)),
    );

    // Tag each item with its source index for "separate" sort mode
    for (let i = 0; i < perSourceResults.length; i++) {
        for (const item of perSourceResults[i]!) {
            item.sourceIndex = i;
        }
    }

    return perSourceResults.flat();
}

async function fetchFirstContent(
    http: HttpClient,
    item: SearchResultItem,
    logger?: Logger,
): Promise<string | null> {
    try {
        if (item.kind === "story") {
            const adapter = getStoryAdapter(item.data.siteName);
            const { chapters } = await adapter.getChapters(http, item.data);
            if (chapters.length === 0) return null;
            const { content } = await adapter.getChapterContent(http, chapters[0]!.url);
            return stripHtml(content);
        } else {
            const adapter = getAdapter(item.data.siteName);
            const { posts } = await adapter.getPosts(http, item.data, 1);
            const firstPost = posts.find((p) => p.ordinal === 1);
            if (!firstPost) return null;
            return stripHtml(firstPost.content);
        }
    } catch (err) {
        logger?.warn("Failed to fetch content for keyword matching", {
            url: item.data.url,
            error: String(err),
        });
        return null;
    }
}

export async function executeSearch(
    config: ScraperConfig,
    http: HttpClient,
    options: SearchOptions,
    logger?: Logger,
): Promise<ScoredSearchResult[]> {
    // Collect raw items from all sources
    logger?.info("Collecting items from sources", { sourceCount: options.sources.length });
    const items = await collectItems(http, options.sources, options, logger);
    logger?.info("Collected items", { count: items.length });

    // Build list status map
    const listsData = loadLists(config.lists.file);
    const statusMap = buildStatusMap(listsData);

    const results: ScoredSearchResult[] = [];

    for (const item of items) {
        const url = getItemUrl(item);
        const listStatus = statusMap.get(url);

        // Stage 1: List filter
        if (listStatus === "ignored" && !options.showIgnored) continue;
        if (options.favoritesOnly && listStatus !== "favorites") continue;

        // Stage 2: Tag filter
        let tagScore = 0;
        if (options.tags && options.tags.length > 0) {
            const itemTags = getItemTags(item);
            const tagResult = fuzzyMatchTags(itemTags, options.tags, options.tagMode);
            if (!tagResult.pass) continue;
            tagScore = tagResult.score;
        }

        // Stage 3: Keyword filter
        let keywordScore = 0;
        if (options.keywords && options.keywords.length > 0) {
            // Phase A: title + summary
            const searchText = getItemSearchText(item);
            const kwResult = fuzzyMatchKeywords(searchText, options.keywords);

            if (kwResult.pass) {
                keywordScore = kwResult.score;
            } else if (options.fetchContent) {
                // Phase B: fetch first chapter/post content
                logger?.debug("Fetching content for keyword matching", {
                    title: getItemTitle(item),
                });
                const content = await fetchFirstContent(http, item, logger);
                if (content) {
                    const contentResult = fuzzyMatchKeywords([content], options.keywords);
                    if (contentResult.pass) {
                        keywordScore = contentResult.score;
                        item.contentPreview = content.slice(0, 200);
                    }
                }
            }

            if (keywordScore === 0) continue;
        }

        results.push({
            item,
            tagScore,
            keywordScore,
            listStatus,
        });
    }

    // Sort results based on sort mode
    if (options.sortMode === "separate") {
        // Group by source, then by date within each group
        results.sort((a, b) => {
            const si = (a.item.sourceIndex ?? 0) - (b.item.sourceIndex ?? 0);
            if (si !== 0) return si;
            const dateA = a.item.data.lastUpdated?.getTime() ?? 0;
            const dateB = b.item.data.lastUpdated?.getTime() ?? 0;
            return dateB - dateA;
        });
    } else {
        // "mixed" — interleave all results by date, score as tiebreaker
        results.sort((a, b) => {
            const dateA = a.item.data.lastUpdated?.getTime() ?? 0;
            const dateB = b.item.data.lastUpdated?.getTime() ?? 0;
            if (dateB !== dateA) return dateB - dateA;
            return b.tagScore + b.keywordScore - (a.tagScore + a.keywordScore);
        });
    }

    logger?.info("Search complete", { results: results.length });
    return results;
}

export function formatSearchResult(result: ScoredSearchResult, showUrls?: boolean): string {
    const { item, listStatus } = result;
    const prefix = getListPrefix(listStatus);
    const kind = item.kind === "thread" ? "[thread]" : "[story]";
    const site = item.data.siteName;
    const id = item.kind === "thread" ? item.data.threadId : item.data.storyId;
    const updated = item.data.lastUpdated?.toISOString() ?? "unknown";
    const title = item.data.title;
    const author = item.data.author;

    let extra = "";
    if (item.kind === "story" && item.data.wordCount) {
        extra = `${item.data.wordCount}w`;
    }

    const allTags = getItemTags(item);
    const tagStr = allTags.length > 0 ? `[${allTags.join(", ")}]` : "";

    const url = showUrls ? item.data.url : "";
    const parts = [prefix + kind, site, id, updated, title, author, extra, tagStr, url].filter(
        Boolean,
    );
    return parts.join("\t");
}
