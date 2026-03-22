export type SiteType = "forum" | "story";

export interface Subforum {
    siteName: string;
    subforumId: string;
    name: string;
    url: string;
}

export interface Thread {
    siteName: string;
    subforumId: string;
    threadId: string;
    title: string;
    author: string;
    url: string;
    lastUpdated: Date | null;
    isSticky: boolean;
    tags: string[];
    fandoms: string[];
    genres: string[];
    characters: string[];
}

export interface Post {
    siteName: string;
    postId: string;
    threadId: string;
    author: string;
    content: string;
    postedAt: Date | null;
    isStoryPost: boolean;
    ordinal: number;
}

export interface Story {
    siteName: string;
    storyId: string;
    title: string;
    author: string;
    url: string;
    summary: string;
    lastUpdated: Date | null;
    categories: string[];
    tags: string[];
    wordCount?: number;
    rating?: string;
    completionStatus?: string;
    kudos?: number;
    hits?: number;
    bookmarks?: number;
    commentCount?: number;
    language?: string;
    relationships?: string[];
    characters?: string[];
    fandoms?: string[];
    chapterCount?: string;
}

export interface Comment {
    siteName: string;
    commentId: string;
    storyId: string;
    author: string;
    content: string;
    postedAt: Date | null;
}
