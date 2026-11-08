import type { ForumAdapter } from "./forums/base";
import type { HttpClient } from "./http";
import type { Logger } from "./logger";
import type { Thread, Threadmark } from "./models";

let warnedEditCheck = false;

export function threadmarkActivity(tm: Threadmark): Date | null {
    const times = [tm.createdAt, tm.editedAt].filter((d): d is Date => d !== null);
    if (times.length === 0) return null;
    return new Date(Math.max(...times.map((d) => d.getTime())));
}

export function threadSortDate(thread: Thread): Date | null {
    return thread.lastThreadmarkAt ?? thread.lastUpdated;
}

/**
 * Keep only threads with threadmark activity (created or edited) at or after the cutoff.
 * Sets threadmarks/lastThreadmarkAt on kept threads. Adapters without threadmark
 * support return the threads unchanged.
 */
export async function applyThreadmarkFilter(
    http: HttpClient,
    adapter: ForumAdapter,
    threads: Thread[],
    cutoff: Date | null,
    opts: { checkEdits: boolean },
    logger?: Logger,
): Promise<Thread[]> {
    if (!adapter.getThreadmarks) return threads;
    const getThreadmarks = adapter.getThreadmarks.bind(adapter);

    const withMarks = threads.filter((t) => {
        if (!t.hasThreadmarks) {
            logger?.debug("Dropping thread (no threadmarks)", { thread: t.title });
        }
        return t.hasThreadmarks;
    });

    const checked = await Promise.all(
        withMarks.map(async (t): Promise<Thread | null> => {
            try {
                let marks: Threadmark[];
                try {
                    marks = await getThreadmarks(http, t, opts);
                } catch (err) {
                    if (!opts.checkEdits) throw err;
                    if (!warnedEditCheck) {
                        warnedEditCheck = true;
                        console.error(
                            `Warning: threadmark edit check failed (${String(err)}). Likely bot-blocked; falling back to threadmark creation dates only. Further failures are logged but not repeated here.`,
                        );
                    }
                    logger?.warn("Edit check failed, using threadmark creation dates only", {
                        thread: t.title,
                        error: String(err),
                    });
                    marks = await getThreadmarks(http, t, { checkEdits: false });
                }
                t.threadmarks = marks;
                const activity = marks.map(threadmarkActivity).filter((d): d is Date => d !== null);
                t.lastThreadmarkAt =
                    activity.length > 0
                        ? new Date(Math.max(...activity.map((d) => d.getTime())))
                        : null;
                if (cutoff && (!t.lastThreadmarkAt || t.lastThreadmarkAt < cutoff)) {
                    logger?.debug("Dropping thread (no threadmark activity in range)", {
                        thread: t.title,
                        lastThreadmarkAt: t.lastThreadmarkAt?.toISOString() ?? null,
                    });
                    return null;
                }
                return t;
            } catch (err) {
                logger?.warn("Failed to fetch threadmarks", {
                    thread: t.title,
                    error: String(err),
                });
                return null;
            }
        }),
    );

    return checked.filter((t): t is Thread => t !== null);
}

let warnedCategories = false;

/**
 * Fetch per-category word counts for threads with threadmarks. Failures are non-fatal:
 * the thread keeps its list-page main-category wordCount.
 */
export async function applyThreadmarkCategories(
    http: HttpClient,
    adapter: ForumAdapter,
    threads: Thread[],
    logger?: Logger,
): Promise<void> {
    if (!adapter.getThreadmarkCategories) return;
    const getCategories = adapter.getThreadmarkCategories.bind(adapter);

    await Promise.all(
        threads
            .filter((t) => t.hasThreadmarks)
            .map(async (t) => {
                try {
                    t.threadmarkCategories = await getCategories(http, t);
                } catch (err) {
                    if (!warnedCategories) {
                        warnedCategories = true;
                        console.error(
                            `Warning: threadmark category fetch failed (${String(err)}); showing main word count only. Further failures are logged but not repeated here.`,
                        );
                    }
                    logger?.warn("Failed to fetch threadmark categories", {
                        thread: t.title,
                        error: String(err),
                    });
                }
            }),
    );
}
