import { CACHE_FILES, cacheRead, cacheWrite, cacheExists } from "../cache.ts";

/**
 * Insert markdown image references below each line containing a [HH:MM:SS] timestamp
 * that has a matching snapshot.
 */
export async function enrichSummary(
  cacheDir: string,
  frameMap: Map<string, string>,
  verbose: boolean,
): Promise<string> {
  if (await cacheExists(cacheDir, CACHE_FILES.summaryWithSnapshots)) {
    console.error("Enriching summary... cached, skipping");
    return cacheRead(cacheDir, CACHE_FILES.summaryWithSnapshots);
  }

  const summary = await cacheRead(cacheDir, CACHE_FILES.summary);

  if (frameMap.size === 0) {
    if (verbose) console.error("[enrich-summary] no frames to insert");
    return summary;
  }

  console.error("Enriching summary with snapshots...");

  const lines = summary.split("\n");
  const enrichedLines: string[] = [];

  for (const line of lines) {
    enrichedLines.push(line);

    // Find all timestamps in this line and insert images for ones we have frames for
    const matches = line.matchAll(/\[(\d{2}:\d{2}:\d{2})\]/g);
    for (const match of matches) {
      const ts = match[1];
      const framePath = frameMap.get(ts);
      if (framePath) {
        enrichedLines.push("");
        enrichedLines.push(`![Video frame at ${ts}](${framePath})`);
        enrichedLines.push("");
      }
    }
  }

  const enriched = enrichedLines.join("\n");
  await cacheWrite(cacheDir, CACHE_FILES.summaryWithSnapshots, enriched);

  if (verbose) console.error("[enrich-summary] wrote summary-with-snapshots.md");
  return enriched;
}
