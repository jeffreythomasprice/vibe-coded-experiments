import type { HttpClient } from "../http";
import type { Story, Comment } from "../models";

export interface ChapterListResult {
    chapters: Array<{ id: string; title: string; url: string }>;
    hasNext: boolean;
}

export interface StoryAdapter {
    readonly siteName: string;
    readonly baseUrl: string;
    readonly antiScraperDelayMs?: number;

    getStoryList(
        http: HttpClient,
        category: string,
        page?: number,
    ): Promise<{ stories: Story[]; hasNext: boolean }>;

    getChapters(http: HttpClient, story: Story): Promise<ChapterListResult>;

    getChapterContent(
        http: HttpClient,
        chapterUrl: string,
    ): Promise<{ content: string; comments: Comment[] }>;

    getComments(
        http: HttpClient,
        chapterUrl: string,
        page?: number,
    ): Promise<{ comments: Comment[]; hasNext: boolean }>;
}
