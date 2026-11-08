import type { Thread } from "./models";

/** Parse "590", "1,234", "3.3k", "1.8m" into an approximate number. */
export function parseWordCount(text: string): number | undefined {
    const m = text.trim().match(/^([\d.,]+)\s*([km])?$/i);
    if (!m) return undefined;
    const n = parseFloat(m[1]!.replace(/,/g, ""));
    if (Number.isNaN(n)) return undefined;
    const suffix = m[2]?.toLowerCase();
    const mult = suffix === "m" ? 1_000_000 : suffix === "k" ? 1_000 : 1;
    return Math.round(n * mult);
}

/** Format like the forums do: 590, 3.3k, 123k, 1.8m. */
export function formatWordCount(n: number | undefined): string {
    if (n === undefined) return "?";
    if (n < 1000) return String(n);
    if (n < 1_000_000) {
        const k = n / 1000;
        return `${k < 10 ? Math.round(k * 10) / 10 : Math.round(k)}k`;
    }
    return `${Math.round(n / 100_000) / 10}m`;
}

/** "123k / 13k / 56k" for multi-category threads, a single figure otherwise, "" if unknown. */
export function formatThreadWords(thread: Thread): string {
    const cats = thread.threadmarkCategories;
    if (cats && cats.length >= 2) {
        return cats.map((c) => formatWordCount(c.words)).join(" / ");
    }
    const single = thread.wordCount ?? cats?.[0]?.words;
    return single === undefined ? "" : formatWordCount(single);
}
