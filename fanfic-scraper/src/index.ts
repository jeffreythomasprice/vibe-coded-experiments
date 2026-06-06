import { Command } from "commander";
import { loadConfig, configToToml } from "./config";
import { Logger } from "./logger";
import { getAdapter, getStoryAdapter, getTargets, createHttpClient, detectSite } from "./scraper";
import { parseDuration } from "./config";
import {
    loadLists,
    saveLists,
    addToList,
    removeFromList,
    buildStatusMap,
    getListPrefix,
    type ListKind,
} from "./lists";
import { resolveSearchSources, executeSearch, formatSearchResult } from "./search";

const program = new Command();

program
    .name("fanfic-scraper")
    .description("Scrape fanfic forums and story sites")
    .option("-c, --config <path>", "path to config file")
    .option("-v, --verbose", "enable verbose logging (also log to stdout)")
    .option("--no-cache", "disable HTTP cache");

function createLogger(opts: { config?: string; verbose?: boolean }): Logger {
    const config = loadConfig(opts.config);
    return new Logger({
        dir: config.logging.dir,
        verbose: opts.verbose,
    });
}

program
    .command("config")
    .description("Print the effective configuration as TOML")
    .action(() => {
        const opts = program.opts();
        const config = loadConfig(opts.config);
        console.log(configToToml(config));
    });

program
    .command("targets")
    .description("List all available scrape targets and their types")
    .action(() => {
        const targets = getTargets();
        for (const t of targets) {
            console.log(`${t.name}\t${t.type}\t${t.baseUrl}`);
        }
    });

program
    .command("subforums")
    .description("List subforums for a site")
    .argument("<site>", "site name (e.g. spacebattles)")
    .action(async (site: string) => {
        const opts = program.opts();
        const config = loadConfig(opts.config);
        const logger = createLogger(opts);

        logger.info("Listing subforums", { site });

        const http = createHttpClient(config, {
            noCache: !opts.cache,
            logger,
        });
        const adapter = getAdapter(site);
        const subforums = await adapter.getSubforums(http);

        logger.info("Found subforums", { site, count: subforums.length });

        for (const sf of subforums) {
            console.log(`${sf.subforumId}\t${sf.name}\t${sf.url}`);
        }
    });

program
    .command("threads")
    .description("List threads in a subforum")
    .argument("<site>", "site name (e.g. spacebattles)")
    .argument("<subforum>", "subforum name or ID")
    .option("-p, --pages <n>", "number of pages to fetch (default: from config)")
    .option(
        "-u, --updated-within <duration>",
        "only show threads updated within duration e.g. 7d (default: from config)",
    )
    .option("--show-ignored", "include ignored threads in output")
    .option("--favorites-only", "only show favorited threads")
    .option("-l, --show-urls", "show thread URLs")
    .action(
        async (
            site: string,
            subforumArg: string,
            cmdOpts: {
                pages?: string;
                updatedWithin?: string;
                showIgnored?: boolean;
                favoritesOnly?: boolean;
                showUrls?: boolean;
            },
        ) => {
            const opts = program.opts();
            const config = loadConfig(opts.config);
            const logger = createLogger(opts);

            logger.info("Listing threads", { site, subforum: subforumArg });

            const http = createHttpClient(config, {
                noCache: !opts.cache,
                logger,
            });
            const adapter = getAdapter(site);
            const allSubforums = await adapter.getSubforums(http);

            const subforum = allSubforums.find(
                (sf) =>
                    sf.subforumId === subforumArg ||
                    sf.name.toLowerCase() === subforumArg.toLowerCase(),
            );

            if (!subforum) {
                logger.error("Subforum not found", {
                    query: subforumArg,
                    available: allSubforums.map((sf) => sf.name),
                });
                console.error(`Subforum not found: ${subforumArg}`);
                console.error(
                    `Available: ${allSubforums.map((sf) => `${sf.name} (${sf.subforumId})`).join(", ")}`,
                );
                process.exit(1);
            }

            const maxPages = cmdOpts.pages
                ? parseInt(cmdOpts.pages, 10)
                : config.subforums.maxPages;
            const updatedWithinStr = cmdOpts.updatedWithin ?? config.subforums.updatedWithin;
            const cutoff = updatedWithinStr
                ? new Date(Date.now() - parseDuration(updatedWithinStr))
                : null;

            const listsData = loadLists(config.lists.file);
            const statusMap = buildStatusMap(listsData);
            let totalThreads = 0;

            for (let page = 1; page <= maxPages; page++) {
                logger.debug("Fetching thread list page", {
                    subforum: subforum.name,
                    page,
                });

                const { threads, hasNext } = await adapter.getThreadList(http, subforum, page);

                let pagePastCutoff = false;

                for (const t of threads) {
                    if (!t.isSticky && cutoff && t.lastUpdated && t.lastUpdated < cutoff) {
                        logger.debug("Thread older than cutoff", {
                            thread: t.title,
                            lastUpdated: t.lastUpdated.toISOString(),
                            cutoff: cutoff.toISOString(),
                        });
                        pagePastCutoff = true;
                    }

                    const status = statusMap.get(t.url);
                    if (status === "ignored" && !cmdOpts.showIgnored) continue;
                    if (cmdOpts.favoritesOnly && status !== "favorites") continue;

                    totalThreads++;
                    const prefix = getListPrefix(status);
                    const updated = t.lastUpdated?.toISOString() ?? "unknown";
                    const tagStr = t.tags.length > 0 ? `\t[${t.tags.join(", ")}]` : "";
                    const urlStr = cmdOpts.showUrls ? `\n    ${t.url}` : "";
                    console.log(
                        `${prefix}${t.threadId}\t${updated}\t${t.title}\t${t.author}${tagStr}${urlStr}`,
                    );
                }

                if (pagePastCutoff || !hasNext) {
                    if (pagePastCutoff) {
                        logger.debug("Stopping pagination: non-sticky thread exceeded cutoff");
                    }
                    break;
                }
            }

            logger.info("Thread listing complete", {
                subforum: subforum.name,
                threadsFound: totalThreads,
            });
        },
    );

program
    .command("posts")
    .description("Fetch posts from a thread")
    .argument("<site>", "site name (e.g. spacebattles)")
    .argument("<thread-url>", "full thread URL")
    .option("-p, --pages <n>", "number of pages to fetch (default: from config)")
    .action(async (site: string, threadUrl: string, cmdOpts: { pages?: string }) => {
        const opts = program.opts();
        const config = loadConfig(opts.config);
        const logger = createLogger(opts);

        logger.info("Fetching posts", { site, threadUrl });

        const http = createHttpClient(config, {
            noCache: !opts.cache,
            logger,
        });
        const adapter = getAdapter(site);

        // Build a minimal thread object from the URL
        const idMatch = threadUrl.match(/\.(\d+)\/?$/);
        if (!idMatch) {
            logger.error("Could not extract thread ID from URL", { threadUrl });
            console.error("Could not extract thread ID from URL");
            process.exit(1);
        }

        console.log(threadUrl);

        const thread = {
            siteName: site,
            subforumId: "",
            threadId: idMatch[1]!,
            title: "",
            author: "",
            url: threadUrl,
            lastUpdated: null,
            isSticky: false,
            tags: [],
            fandoms: [],
            genres: [],
            characters: [],
        };

        const maxPages = cmdOpts.pages ? parseInt(cmdOpts.pages, 10) : config.threads.maxPages;
        let totalPosts = 0;

        for (let page = 1; page <= maxPages; page++) {
            logger.debug("Fetching posts page", {
                threadId: thread.threadId,
                page,
            });

            const { posts, hasNext } = await adapter.getPosts(http, thread, page);
            totalPosts += posts.length;

            for (const p of posts) {
                const time = p.postedAt?.toISOString() ?? "unknown";
                const story = p.isStoryPost ? "[STORY]" : "";
                // Truncate content for display
                const preview = p.content
                    .replace(/<[^>]+>/g, "")
                    .slice(0, 120)
                    .replace(/\s+/g, " ");
                console.log(`${p.postId}\t${p.author}\t${time}\t${story}\t${preview}`);
            }

            if (!hasNext) break;
        }

        logger.info("Posts fetch complete", {
            threadId: thread.threadId,
            postsFound: totalPosts,
        });
    });

program
    .command("stories")
    .description("List stories from a story site by canonical tag")
    .argument("<site>", "site name (e.g. ao3)")
    .argument("<tag>", "canonical tag name (e.g. 'Parahumans Series - Wildbow')")
    .option("-p, --pages <n>", "number of pages to fetch (default: from config)")
    .option(
        "-u, --updated-within <duration>",
        "only show stories updated within duration e.g. 7d (default: from config)",
    )
    .option("--show-ignored", "include ignored stories in output")
    .option("--favorites-only", "only show favorited stories")
    .option("-l, --show-urls", "show story URLs")
    .action(
        async (
            site: string,
            tag: string,
            cmdOpts: {
                pages?: string;
                updatedWithin?: string;
                showIgnored?: boolean;
                favoritesOnly?: boolean;
                showUrls?: boolean;
            },
        ) => {
            const opts = program.opts();
            const config = loadConfig(opts.config);
            const logger = createLogger(opts);

            logger.info("Listing stories", { site, tag });

            const http = createHttpClient(config, {
                noCache: !opts.cache,
                logger,
            });
            const adapter = getStoryAdapter(site);

            const maxPages = cmdOpts.pages ? parseInt(cmdOpts.pages, 10) : config.stories.maxPages;
            const updatedWithinStr = cmdOpts.updatedWithin ?? config.stories.updatedWithin;
            const cutoff = updatedWithinStr
                ? new Date(Date.now() - parseDuration(updatedWithinStr))
                : null;

            const listsData = loadLists(config.lists.file);
            const statusMap = buildStatusMap(listsData);
            let totalStories = 0;

            for (let page = 1; page <= maxPages; page++) {
                logger.debug("Fetching story list page", { tag, page });

                const { stories, hasNext } = await adapter.getStoryList(http, tag, page);

                let pagePastCutoff = false;

                for (const s of stories) {
                    if (cutoff && s.lastUpdated && s.lastUpdated < cutoff) {
                        logger.debug("Story older than cutoff", {
                            story: s.title,
                            lastUpdated: s.lastUpdated.toISOString(),
                            cutoff: cutoff.toISOString(),
                        });
                        pagePastCutoff = true;
                        continue;
                    }

                    const status = statusMap.get(s.url);
                    if (status === "ignored" && !cmdOpts.showIgnored) continue;
                    if (cmdOpts.favoritesOnly && status !== "favorites") continue;

                    totalStories++;
                    const prefix = getListPrefix(status);
                    const updated = s.lastUpdated?.toISOString() ?? "unknown";
                    const words = s.wordCount ?? "?";
                    const tagStr = s.tags.length > 0 ? `\t[${s.tags.join(", ")}]` : "";
                    const urlStr = cmdOpts.showUrls ? `\n    ${s.url}` : "";
                    console.log(
                        `${prefix}${s.storyId}\t${words}\t${updated}\t${s.title}\t${s.author}${tagStr}${urlStr}`,
                    );
                }

                if (pagePastCutoff || !hasNext) {
                    if (pagePastCutoff) {
                        logger.debug("Stopping pagination: story exceeded cutoff");
                    }
                    break;
                }
            }

            logger.info("Story listing complete", {
                tag,
                storiesFound: totalStories,
            });
        },
    );

program
    .command("chapters")
    .description("List chapters of a work")
    .argument("<site>", "site name (e.g. ao3)")
    .argument("<work-url>", "full work URL")
    .action(async (site: string, workUrl: string) => {
        const opts = program.opts();
        const config = loadConfig(opts.config);
        const logger = createLogger(opts);

        logger.info("Listing chapters", { site, workUrl });

        const http = createHttpClient(config, {
            noCache: !opts.cache,
            logger,
        });
        const adapter = getStoryAdapter(site);

        const idMatch = workUrl.match(/\/works\/(\d+)/);
        if (!idMatch) {
            console.error("Could not extract work ID from URL");
            process.exit(1);
        }

        const story = {
            siteName: site,
            storyId: idMatch[1]!,
            title: "",
            author: "",
            url: workUrl,
            summary: "",
            lastUpdated: null,
            categories: [],
            tags: [],
        };

        console.log(workUrl);

        const { chapters } = await adapter.getChapters(http, story);

        for (const ch of chapters) {
            console.log(`${ch.id}\t${ch.title}\t${ch.url}`);
        }

        logger.info("Chapter listing complete", {
            workId: story.storyId,
            chapterCount: chapters.length,
        });
    });

program
    .command("content")
    .description("Fetch chapter content")
    .argument("<site>", "site name (e.g. ao3)")
    .argument("<chapter-url>", "full chapter URL")
    .action(async (site: string, chapterUrl: string) => {
        const opts = program.opts();
        const config = loadConfig(opts.config);
        const logger = createLogger(opts);

        logger.info("Fetching chapter content", { site, chapterUrl });

        const http = createHttpClient(config, {
            noCache: !opts.cache,
            logger,
        });
        const adapter = getStoryAdapter(site);

        const { content } = await adapter.getChapterContent(http, chapterUrl);
        console.log(content);

        logger.info("Chapter content fetched", { chapterUrl });
    });

program
    .command("comments")
    .description("Fetch comments on a chapter")
    .argument("<site>", "site name (e.g. ao3)")
    .argument("<chapter-url>", "full chapter URL")
    .option("-p, --pages <n>", "number of pages to fetch", "10")
    .action(async (site: string, chapterUrl: string, cmdOpts: { pages: string }) => {
        const opts = program.opts();
        const config = loadConfig(opts.config);
        const logger = createLogger(opts);

        logger.info("Fetching comments", { site, chapterUrl });

        const http = createHttpClient(config, {
            noCache: !opts.cache,
            logger,
        });
        const adapter = getStoryAdapter(site);

        console.log(chapterUrl);

        const maxPages = parseInt(cmdOpts.pages, 10);
        let totalComments = 0;

        for (let page = 1; page <= maxPages; page++) {
            logger.debug("Fetching comments page", { chapterUrl, page });

            const { comments, hasNext } = await adapter.getComments(http, chapterUrl, page);
            totalComments += comments.length;

            for (const c of comments) {
                const time = c.postedAt?.toISOString() ?? "unknown";
                const preview = c.content
                    .replace(/<[^>]+>/g, "")
                    .slice(0, 120)
                    .replace(/\s+/g, " ");
                console.log(`${c.commentId}\t${c.author}\t${time}\t${preview}`);
            }

            if (!hasNext) break;
        }

        logger.info("Comments fetch complete", {
            chapterUrl,
            commentsFound: totalComments,
        });
    });

program
    .command("search")
    .description("Search across multiple sites with filters")
    .argument("[sources...]", 'source specs like spacebattles:"Creative Writing" ao3:"tag"')
    .option("--sites <sites>", "comma-separated site names to filter sources")
    .option("--tags <tags>", "comma-separated tags for fuzzy matching")
    .option("--tag-mode <mode>", "any or all", "any")
    .option("--keywords <keywords>", "comma-separated keywords for fuzzy matching")
    .option("--fetch-content", "fetch first post/chapter for keyword matching")
    .option(
        "-u, --updated-within <duration>",
        "only show items updated within duration e.g. 7d (default: from config)",
    )
    .option("-p, --pages <n>", "max pages per source (default: from config)")
    .option("--show-ignored", "include ignored items")
    .option("--favorites-only", "only show favorites")
    .option(
        "--sort-mode <mode>",
        "mixed (interleave by date) or separate (group by source)",
        "mixed",
    )
    .option("-l, --show-urls", "show item URLs")
    .action(
        async (
            sourceArgs: string[],
            cmdOpts: {
                sites?: string;
                tags?: string;
                tagMode: string;
                keywords?: string;
                fetchContent?: boolean;
                updatedWithin?: string;
                pages?: string;
                sortMode: string;
                showIgnored?: boolean;
                favoritesOnly?: boolean;
                showUrls?: boolean;
            },
        ) => {
            const opts = program.opts();
            const config = loadConfig(opts.config);
            const logger = createLogger(opts);

            const siteFilter = cmdOpts.sites?.split(",").map((s) => s.trim());
            const sources = resolveSearchSources(sourceArgs, config.search.sources, siteFilter);

            if (sources.length === 0) {
                console.error(
                    "No search sources specified. Provide sources as arguments or configure them in config.toml [search.sources].",
                );
                process.exit(1);
            }

            logger.info("Starting search", {
                sources: sources.length,
                tags: cmdOpts.tags,
                keywords: cmdOpts.keywords,
            });

            const http = createHttpClient(config, {
                noCache: !opts.cache,
                logger,
            });

            const results = await executeSearch(
                config,
                http,
                {
                    sources,
                    sites: siteFilter,
                    tags: cmdOpts.tags?.split(",").map((s) => s.trim()),
                    tagMode: cmdOpts.tagMode as "any" | "all",
                    keywords: cmdOpts.keywords?.split(",").map((s) => s.trim()),
                    fetchContent: cmdOpts.fetchContent ?? false,
                    updatedWithin: cmdOpts.updatedWithin ?? config.search.updatedWithin,
                    showIgnored: cmdOpts.showIgnored ?? false,
                    favoritesOnly: cmdOpts.favoritesOnly ?? false,
                    maxPages: cmdOpts.pages ? parseInt(cmdOpts.pages, 10) : config.search.maxPages,
                    sortMode: cmdOpts.sortMode as "mixed" | "separate",
                },
                logger,
            );

            for (const result of results) {
                console.log(formatSearchResult(result, cmdOpts.showUrls));
            }

            const threadCount = results.filter((r) => r.item.kind === "thread").length;
            const storyCount = results.filter((r) => r.item.kind === "story").length;
            console.error(
                `Found ${results.length} results (${threadCount} threads, ${storyCount} stories) across ${sources.length} sources`,
            );
        },
    );

function makeListCommands(kind: ListKind): Command {
    const label = kind === "favorites" ? "favorites" : "ignore";
    const cmd = new Command(kind === "favorites" ? "fav" : "ignore").description(
        `Manage ${label} list`,
    );

    cmd.command("list")
        .description(`Show all ${label} entries`)
        .action(() => {
            const opts = program.opts();
            const config = loadConfig(opts.config);
            const data = loadLists(config.lists.file);
            const entries = data[kind];

            if (entries.length === 0) {
                console.log(`No ${label} entries.`);
                return;
            }

            for (const entry of entries) {
                console.log(`${entry.site}\t${entry.url}\t${entry.added_at}`);
            }
        });

    cmd.command("add")
        .description(`Add a URL to the ${label} list`)
        .argument("<url>", "thread or story URL")
        .action((url: string) => {
            const opts = program.opts();
            const config = loadConfig(opts.config);

            const detected = detectSite(url);
            if (!detected) {
                console.error(`Could not detect site for URL: ${url}`);
                console.error(
                    `Known sites: ${getTargets()
                        .map((t) => `${t.name} (${t.baseUrl})`)
                        .join(", ")}`,
                );
                process.exit(1);
            }

            const data = loadLists(config.lists.file);
            const result = addToList(data, kind, url, detected.site);

            if (!result.ok) {
                console.error(result.warning);
                process.exit(1);
            }

            saveLists(config.lists.file, data);
            console.log(`Added to ${label}: ${url}`);
        });

    cmd.command("remove")
        .description(`Remove a URL from the ${label} list`)
        .argument("<url>", "thread or story URL")
        .action((url: string) => {
            const opts = program.opts();
            const config = loadConfig(opts.config);

            const data = loadLists(config.lists.file);
            const removed = removeFromList(data, kind, url);

            if (!removed) {
                console.error(`URL not found in ${label} list: ${url}`);
                process.exit(1);
            }

            saveLists(config.lists.file, data);
            console.log(`Removed from ${label}: ${url}`);
        });

    return cmd;
}

program.addCommand(makeListCommands("favorites"));
program.addCommand(makeListCommands("ignored"));

program.parse();
