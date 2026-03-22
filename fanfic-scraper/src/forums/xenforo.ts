import * as cheerio from "cheerio";
import type { HttpClient } from "../http";
import type { Subforum, Thread, Post } from "../models";
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

            threads.push({
                siteName: this.siteName,
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
}
