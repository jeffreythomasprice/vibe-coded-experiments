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
  url: string;
  lastUpdated: Date | null;
  isSticky: boolean;
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
}

export interface Comment {
  siteName: string;
  commentId: string;
  storyId: string;
  author: string;
  content: string;
  postedAt: Date | null;
}
