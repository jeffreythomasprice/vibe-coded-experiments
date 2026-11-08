import * as cheerio from "cheerio";
import type { HttpClient } from "../http";
import type { Subforum, Thread, Post, Threadmark, ThreadmarkCategory } from "../models";
import { parseWordCount } from "../wordcount";
import type { ForumAdapter, ThreadListResult, PostListResult } from "./base";

export interface XenForoConfig {
    siteName: string;
    baseUrl: string;
    antiScraperDelayMs?: number;
    postsPerPage?: number;
    tagCategories?: {
        fandom?: string;
        genre?: string;
        character?: string;
    };
}

export class XenForoAdapter implements ForumAdapter {
    readonly siteName: string;
    readonly baseUrl: string;
    readonly antiScraperDelayMs?: number;
    private readonly postsPerPage: number;
    private readonly tagCategories: NonNullable<XenForoConfig["tagCategories"]>;

    constructor(config: XenForoConfig) {
        this.siteName = config.siteName;
        this.baseUrl = config.baseUrl;
        this.antiScraperDelayMs = config.antiScraperDelayMs;
        this.postsPerPage = config.postsPerPage ?? 25;
        this.tagCategories = config.tagCategories ?? {};
    }

    async getSubforums(http: HttpClient): Promise<Subforum[]> {
        const res = await http.get(`${this.baseUrl}/`);
        const $ = cheerio.load(res.text());
        const subforums: Subforum[] = [];

        $("div.node--forum").each((_, el) => {
            const classes = $(el).attr("class") ?? "";
            const idMatch = classes.match(/node--id(\d+)/);
            if (!idMatch) return;

            const subforumId = idMatch[1]!;
            const titleLink = $(el).find("h3.node-title a").first();
            const name = titleLink.text().trim();
            const href = titleLink.attr("href");

            if (!name || !href) return;

            subforums.push({
                siteName: this.siteName,
                subforumId,
                name,
                url: `${this.baseUrl}${href}`,
            });
        });

        return subforums;
    }

    async getThreadList(http: HttpClient, subforum: Subforum, page = 1): Promise<ThreadListResult> {
        const url = page === 1 ? subforum.url : `${subforum.url}page-${page}`;
        const res = await http.get(url);
        const $ = cheerio.load(res.text());
        const threads: Thread[] = [];

        $("div.structItem--thread.js-inlineModContainer").each((_, el) => {
            const $el = $(el);

            const titleLink = $el.find("div.structItem-title a[href*='/threads/']").first();
            const title = titleLink.text().trim();
            const href = titleLink.attr("href") ?? "";

            const threadIdMatch = href.match(/\.(\d+)\/?$/);
            if (!threadIdMatch) return;

            const threadId = threadIdMatch[1]!;

            const latestTime = $el.find("time.structItem-latestDate").first();
            const datetime = latestTime.attr("datetime");
            const lastUpdated = datetime ? new Date(datetime) : null;

            const author = $el.find("a.username").first().text().trim();

            const tags: string[] = [];
            const fandoms: string[] = [];
            const genres: string[] = [];
            const characters: string[] = [];

            $el.find("span.structItem-tagBlock a.tagItem").each((_, tagEl) => {
                const $tag = $(tagEl);
                const text = $tag.text().trim();
                if (!text) return;

                const classes = $tag.attr("class") ?? "";
                tags.push(text);

                if (this.tagCategories.fandom && classes.includes(this.tagCategories.fandom))
                    fandoms.push(text);
                else if (this.tagCategories.genre && classes.includes(this.tagCategories.genre))
                    genres.push(text);
                else if (
                    this.tagCategories.character &&
                    classes.includes(this.tagCategories.character)
                )
                    characters.push(text);
            });

            const wordsText = $el
                .find(".structItem-parts a[href$='/threadmarks']")
                .text()
                .match(/Words:\s*([\d.,]+\s*[km]?)/i)?.[1];
            const wordCount = wordsText ? parseWordCount(wordsText) : undefined;

            threads.push({
                siteName: this.siteName,
                wordCount,
                subforumId: subforum.subforumId,
                threadId,
                title,
                author,
                url: `${this.baseUrl}${href}`,
                lastUpdated,
                isSticky: $el.find(".structItem-status--sticky").length > 0,
                tags,
                fandoms,
                genres,
                characters,
                hasThreadmarks: $el.find("a[href$='/threadmarks']").length > 0,
            });
        });

        const hasNext = $("a.pageNav-jump--next").length > 0;

        return { threads, hasNext };
    }

    async getPosts(http: HttpClient, thread: Thread, page = 1): Promise<PostListResult> {
        const url = page === 1 ? thread.url : `${thread.url}page-${page}`;
        const res = await http.get(url);
        const $ = cheerio.load(res.text());
        const posts: Post[] = [];

        $("article.message--post").each((i, el) => {
            const $el = $(el);
            const author = $el.attr("data-author") ?? "";
            const dataContent = $el.attr("data-content") ?? "";
            const postIdMatch = dataContent.match(/post-(\d+)/);
            if (!postIdMatch) return;

            const postId = postIdMatch[1]!;

            const content = $el.find(".message-body .bbWrapper").first().html() ?? "";

            const timeEl = $el.find("time.u-dt").first();
            const datetime = timeEl.attr("datetime");
            const postedAt = datetime ? new Date(datetime) : null;

            const isStoryPost = $el.hasClass("hasThreadmark");

            posts.push({
                siteName: this.siteName,
                postId,
                threadId: thread.threadId,
                author,
                content,
                postedAt,
                isStoryPost,
                ordinal: i + 1 + (page - 1) * this.postsPerPage,
            });
        });

        const hasNext = $("a.pageNav-jump--next").length > 0;

        return { posts, hasNext };
    }

    /** Ordered threadmark categories (tab order) with per-category word totals. */
    async getThreadmarkCategories(http: HttpClient, thread: Thread): Promise<ThreadmarkCategory[]> {
        const base = thread.url.endsWith("/") ? thread.url : `${thread.url}/`;
        const statsRe = /Statistics \(([\d,]+) threadmarks?, ([\d.,]+\s*[km]?) words?\)/i;
        const parseStats = (html: string) => {
            const m = html.match(statsRe);
            return {
                threadmarkCount: m ? parseInt(m[1]!.replace(/,/g, ""), 10) : undefined,
                words: m ? parseWordCount(m[2]!) : undefined,
            };
        };

        const res = await http.get(`${base}threadmarks`);
        if (res.status < 200 || res.status >= 300) {
            throw new Error(`Threadmarks page returned HTTP ${res.status}`);
        }
        const html = res.text();
        const $ = cheerio.load(html);

        const cats: ThreadmarkCategory[] = [];
        $(".block-tabHeader--threadmarkCategoryTabs a.tabs-tab").each((_, el) => {
            const href = $(el).attr("href") ?? "";
            const id = href.match(/threadmark_category=(\d+)/)?.[1] ?? "1";
            cats.push({ id, name: $(el).text().trim() });
        });
        if (cats.length === 0) cats.push({ id: "1", name: "Threadmarks" });

        // The page we fetched shows the first (main) category's statistics.
        Object.assign(cats[0]!, parseStats(html));

        await Promise.all(
            cats.slice(1).map(async (cat) => {
                try {
                    const r = await http.get(`${base}threadmarks?threadmark_category=${cat.id}`);
                    if (r.status < 200 || r.status >= 300) return;
                    Object.assign(cat, parseStats(r.text()));
                } catch {
                    // leave words undefined for this category
                }
            }),
        );

        return cats;
    }

    async getThreadmarks(
        http: HttpClient,
        thread: Thread,
        opts: { checkEdits: boolean },
    ): Promise<Threadmark[]> {
        const base = thread.url.endsWith("/") ? thread.url : `${thread.url}/`;
        const marks = new Map<string, Threadmark>();

        // RSS: newest 10 threadmarks with creation dates and word counts
        const rss = await http.get(`${base}threadmarks.rss?threadmark_category=1`);
        const $rss = cheerio.load(rss.text(), { xmlMode: true });
        $rss("item").each((_, el) => {
            const $el = $rss(el);
            const guid = $el.children("guid").text();
            const postId = guid.match(/posts\/(\d+)/)?.[1];
            if (!postId) return;
            const pub = $el.children("pubDate").text();
            const words = parseInt($el.children("threadmarks\\:words").text(), 10);
            marks.set(postId, {
                postId,
                title: $el.children("title").text().trim(),
                url: $el.children("link").text().trim(),
                createdAt: pub ? new Date(pub) : null,
                editedAt: null,
                words: Number.isNaN(words) ? undefined : words,
            });
        });

        if (opts.checkEdits) {
            // Reader mode: 10 threadmarked posts per page, oldest first, with edit times
            for (let page = 1; ; page++) {
                const url = page === 1 ? `${base}reader/` : `${base}reader/page-${page}`;
                const res = await http.get(url);
                if (res.status < 200 || res.status >= 300) {
                    throw new Error(`Reader page returned HTTP ${res.status}: ${url}`);
                }
                const $ = cheerio.load(res.text());
                if ($("article.message--post").length === 0 && page === 1) {
                    throw new Error(`Reader page had no posts (blocked or empty): ${url}`);
                }
                $("article.message--post").each((_, el) => {
                    const $el = $(el);
                    const postId = ($el.attr("data-content") ?? "").match(/post-(\d+)/)?.[1];
                    if (!postId) return;
                    const edited = $el.find(".message-lastEdit time").first().attr("datetime");
                    const posted = $el.find("time.u-dt").first().attr("datetime");
                    const editedAt = edited ? new Date(edited) : null;
                    const existing = marks.get(postId);
                    if (existing) {
                        existing.editedAt = editedAt;
                    } else {
                        marks.set(postId, {
                            postId,
                            title: "",
                            url: `${this.baseUrl}/posts/${postId}`,
                            createdAt: posted ? new Date(posted) : null,
                            editedAt,
                        });
                    }
                });
                if ($("a.pageNav-jump--next").length === 0) break;
            }
        }

        return [...marks.values()];
    }
}
