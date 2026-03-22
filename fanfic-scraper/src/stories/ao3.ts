import * as cheerio from "cheerio";
import type { HttpClient } from "../http";
import type { Story, Comment } from "../models";
import type { StoryAdapter, ChapterListResult } from "./base";

export class AO3Adapter implements StoryAdapter {
    readonly siteName = "ao3";
    readonly baseUrl = "https://archiveofourown.org";
    readonly antiScraperDelayMs = 2000;

    async getStoryList(
        http: HttpClient,
        category: string,
        page = 1,
    ): Promise<{ stories: Story[]; hasNext: boolean }> {
        const tag = encodeURIComponent(category);
        const url = `${this.baseUrl}/tags/${tag}/works?view_adult=true&page=${page}`;
        const res = await http.get(url);
        const $ = cheerio.load(res.text());
        const stories: Story[] = [];

        $("li.work.blurb.group").each((_, el) => {
            const $el = $(el);

            // Title and story ID
            const titleLink = $el.find("h4.heading a").first();
            const title = titleLink.text().trim();
            const href = titleLink.attr("href") ?? "";
            const idMatch = href.match(/\/works\/(\d+)/);
            if (!idMatch) return;
            const storyId = idMatch[1]!;

            // Author
            const author = $el.find('a[rel="author"]').first().text().trim() || "Anonymous";

            // Summary
            const summary = $el.find("blockquote.userstuff.summary").text().trim();

            // Last updated — prefer the HTML comment with unix timestamp
            const headerHtml = $el.find("div.header.module").html() ?? "";
            const tsMatch = headerHtml.match(/updated_at=(\d+)/);
            let lastUpdated: Date | null = null;
            if (tsMatch) {
                lastUpdated = new Date(parseInt(tsMatch[1]!, 10) * 1000);
            } else {
                // Fallback to the datetime text
                const dateText =
                    $el.find("p.datetime").text().trim() ||
                    $el.find("dd.status").text().trim() ||
                    $el.find("dd.published").text().trim();
                if (dateText) lastUpdated = new Date(dateText);
            }

            // Stats
            const wordCount = parseIntFromText($el.find("dd.words").text());
            const kudos = parseIntFromText($el.find("dd.kudos").text());
            const hits = parseIntFromText($el.find("dd.hits").text());
            const bookmarks = parseIntFromText($el.find("dd.bookmarks a").text());
            const commentCount = parseIntFromText($el.find("dd.comments a").text());
            const chapterCount = $el.find("dd.chapters").text().trim() || undefined;
            const language = $el.find("dd.language").text().trim() || undefined;

            // Completion status — derive from chapter count
            let completionStatus: string | undefined;
            if (chapterCount) {
                const parts = chapterCount.split("/");
                if (parts.length === 2) {
                    completionStatus = parts[1]!.trim() === "?" ? "Work in Progress" : "Complete";
                }
            }

            // Rating — from the required-tags span
            const rating = $el.find("span.rating").text().trim() || undefined;

            // Categories (Gen, F/M, etc.)
            const categoryText = $el.find("span.category").text().trim();
            const categories = categoryText ? categoryText.split(/,\s*/) : [];

            // Fandoms
            const fandoms: string[] = [];
            $el.find(".fandoms a.tag").each((_, a) => {
                fandoms.push($(a).text().trim());
            });

            // Relationships
            const relationships: string[] = [];
            $el.find("li.relationships a.tag").each((_, a) => {
                relationships.push($(a).text().trim());
            });

            // Characters
            const characters: string[] = [];
            $el.find("li.characters a.tag").each((_, a) => {
                characters.push($(a).text().trim());
            });

            // Freeform tags
            const tags: string[] = [];
            $el.find("li.freeforms a.tag").each((_, a) => {
                tags.push($(a).text().trim());
            });

            stories.push({
                siteName: this.siteName,
                storyId,
                title,
                author,
                url: `${this.baseUrl}/works/${storyId}`,
                summary,
                lastUpdated,
                categories,
                tags,
                wordCount,
                rating,
                completionStatus,
                kudos,
                hits,
                bookmarks,
                commentCount,
                language,
                relationships,
                characters,
                fandoms,
                chapterCount,
            });
        });

        const hasNext = $("li.next a").length > 0;

        return { stories, hasNext };
    }

    async getChapters(http: HttpClient, story: Story): Promise<ChapterListResult> {
        const url = `${this.baseUrl}/works/${story.storyId}/navigate?view_adult=true`;
        const res = await http.get(url);
        const $ = cheerio.load(res.text());
        const chapters: Array<{ id: string; title: string; url: string }> = [];

        $("ol.chapter.index.group li a").each((_, el) => {
            const $a = $(el);
            const href = $a.attr("href") ?? "";
            const chapterIdMatch = href.match(/\/chapters\/(\d+)/);
            if (!chapterIdMatch) return;

            chapters.push({
                id: chapterIdMatch[1]!,
                title: $a.text().trim(),
                url: `${this.baseUrl}${href}`,
            });
        });

        // Single-chapter works may not have a navigate listing
        if (chapters.length === 0) {
            chapters.push({
                id: story.storyId,
                title: story.title,
                url: `${this.baseUrl}/works/${story.storyId}?view_adult=true`,
            });
        }

        return { chapters, hasNext: false };
    }

    async getChapterContent(
        http: HttpClient,
        chapterUrl: string,
    ): Promise<{ content: string; comments: Comment[] }> {
        const url = appendViewAdult(chapterUrl);
        const res = await http.get(url);
        const $ = cheerio.load(res.text());

        // Chapter content is in div.userstuff with role="article", or div#chapters .userstuff
        const content =
            $('div.userstuff[role="article"]').html() ??
            $("div#chapters div.userstuff").html() ??
            "";

        return { content, comments: [] };
    }

    async getComments(
        http: HttpClient,
        chapterUrl: string,
        page = 1,
    ): Promise<{ comments: Comment[]; hasNext: boolean }> {
        // AO3 loads comments via show_comments=true&page=N params
        const url = appendParam(appendViewAdult(chapterUrl), "show_comments", "true");
        const pageUrl = page > 1 ? appendParam(url, "comment_page", String(page)) : url;
        const res = await http.get(pageUrl);
        const $ = cheerio.load(res.text());
        const comments: Comment[] = [];

        // Extract story ID from URL
        const storyIdMatch = chapterUrl.match(/\/works\/(\d+)/);
        const storyId = storyIdMatch?.[1] ?? "";

        $('li.comment[id^="comment_"]').each((_, el) => {
            const $el = $(el);
            const rawId = $el.attr("id") ?? "";
            const commentId = rawId.replace("comment_", "");
            if (!commentId || !/^\d+$/.test(commentId)) return;

            const author = $el.find("h4.heading.byline > a").first().text().trim() || "Anonymous";
            const content = $el.find("blockquote.userstuff").first().html() ?? "";

            // Timestamp from structured spans inside span.posted.datetime
            const dtSpan = $el.find("span.posted.datetime").first();
            const day = dtSpan.find("span.date").text().trim();
            const month = dtSpan.find("abbr.month").attr("title") ?? "";
            const year = dtSpan.find("span.year").text().trim();
            const time = dtSpan.find("span.time").text().trim();
            const postedAt = parseAO3CommentDate(`${day} ${month} ${year} ${time} UTC`);

            comments.push({
                siteName: this.siteName,
                commentId,
                storyId,
                author,
                content,
                postedAt,
            });
        });

        const hasNext = $("ol.pagination li.next a").length > 0;

        return { comments, hasNext };
    }
}

function parseIntFromText(text: string): number | undefined {
    const cleaned = text.replace(/,/g, "").trim();
    const n = parseInt(cleaned, 10);
    return isNaN(n) ? undefined : n;
}

function appendViewAdult(url: string): string {
    return appendParam(url, "view_adult", "true");
}

function appendParam(url: string, key: string, value: string): string {
    const sep = url.includes("?") ? "&" : "?";
    return `${url}${sep}${key}=${value}`;
}

// Parses AO3 comment date: "17 October 2025 01:51AM UTC"
function parseAO3CommentDate(text: string): Date | null {
    const match = text.match(/(\d{1,2})\s+(\w+)\s+(\d{4})\s+(\d{1,2}):(\d{2})(AM|PM)\s+UTC/i);
    if (!match) return null;

    const months: Record<string, number> = {
        january: 0,
        february: 1,
        march: 2,
        april: 3,
        may: 4,
        june: 5,
        july: 6,
        august: 7,
        september: 8,
        october: 9,
        november: 10,
        december: 11,
        jan: 0,
        feb: 1,
        mar: 2,
        apr: 3,
        jun: 5,
        jul: 6,
        aug: 7,
        sep: 8,
        oct: 9,
        nov: 10,
        dec: 11,
    };

    const [, day, mon, year, hour, min, ampm] = match;
    const monthIdx = months[mon!.toLowerCase()];
    if (monthIdx === undefined) return null;

    let h = parseInt(hour!, 10);
    if (ampm!.toUpperCase() === "PM" && h !== 12) h += 12;
    if (ampm!.toUpperCase() === "AM" && h === 12) h = 0;

    const d = new Date(
        Date.UTC(parseInt(year!, 10), monthIdx, parseInt(day!, 10), h, parseInt(min!, 10)),
    );
    return isNaN(d.getTime()) ? null : d;
}
