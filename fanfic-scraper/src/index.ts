import { Command } from "commander";
import { loadConfig } from "./config";
import { Logger } from "./logger";
import {
  getAdapter,
  getTargets,
  createHttpClient,
} from "./scraper";
import { parseDuration } from "./config";

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
  .option("-u, --updated-within <duration>", "only show threads updated within duration e.g. 7d (default: from config)")
  .action(async (site: string, subforumArg: string, cmdOpts: { pages?: string; updatedWithin?: string }) => {
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

    const maxPages = cmdOpts.pages ? parseInt(cmdOpts.pages, 10) : config.subforums.maxPages;
    const updatedWithinStr = cmdOpts.updatedWithin ?? config.subforums.updatedWithin;
    const cutoff = updatedWithinStr
      ? new Date(Date.now() - parseDuration(updatedWithinStr))
      : null;

    let totalThreads = 0;
    let done = false;

    for (let page = 1; page <= maxPages && !done; page++) {
      logger.debug("Fetching thread list page", {
        subforum: subforum.name,
        page,
      });

      const { threads, hasNext } = await adapter.getThreadList(
        http,
        subforum,
        page,
      );

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

        totalThreads++;
        const updated = t.lastUpdated?.toISOString() ?? "unknown";
        console.log(`${t.threadId}\t${updated}\t${t.title}`);
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
  });

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

    const thread = {
      siteName: site,
      subforumId: "",
      threadId: idMatch[1]!,
      title: "",
      url: threadUrl,
      lastUpdated: null,
      isSticky: false,
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
        console.log(
          `${p.postId}\t${p.author}\t${time}\t${story}\t${preview}`,
        );
      }

      if (!hasNext) break;
    }

    logger.info("Posts fetch complete", {
      threadId: thread.threadId,
      postsFound: totalPosts,
    });
  });

program.parse();
