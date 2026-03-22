import type { HttpClient } from "../http";
import type { Subforum, Thread, Post } from "../models";

export interface ThreadListResult {
  threads: Thread[];
  hasNext: boolean;
}

export interface PostListResult {
  posts: Post[];
  hasNext: boolean;
}

export interface ForumAdapter {
  readonly siteName: string;
  readonly baseUrl: string;

  getSubforums(http: HttpClient): Promise<Subforum[]>;
  getThreadList(
    http: HttpClient,
    subforum: Subforum,
    page?: number,
  ): Promise<ThreadListResult>;
  getPosts(
    http: HttpClient,
    thread: Thread,
    page?: number,
  ): Promise<PostListResult>;
}
