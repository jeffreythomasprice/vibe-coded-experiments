import { join } from "node:path";
import { mkdir } from "node:fs/promises";
import { CACHE_FILES, cacheRead } from "../cache.ts";
import { ensureBinary, run } from "../utils/subprocess.ts";
import { ProgressBar } from "../utils/progress.ts";

const MAX_FRAMES = 15;

/**
 * Parse all [HH:MM:SS] timestamps from summary text, deduplicated.
 */
export function parseTimestamps(summary: string): string[] {
  const matches = summary.matchAll(/\[(\d{2}:\d{2}:\d{2})\]/g);
  const seen = new Set<string>();
  const timestamps: string[] = [];

  for (const match of matches) {
    const ts = match[1];
    if (!seen.has(ts)) {
      seen.add(ts);
      timestamps.push(ts);
    }
    if (timestamps.length >= MAX_FRAMES) break;
  }

  return timestamps;
}

/**
 * Extract video frames at each timestamp using ffmpeg.
 * Returns a Map of timestamp -> snapshot file path (relative to cacheDir).
 */
export async function extractFrames(
  cacheDir: string,
  verbose: boolean,
): Promise<Map<string, string>> {
  const summary = await cacheRead(cacheDir, CACHE_FILES.summary);
  const timestamps = parseTimestamps(summary);

  if (timestamps.length === 0) {
    if (verbose) console.error("[extract-frames] no timestamps found in summary");
    return new Map();
  }

  const ffmpeg = ensureBinary("ffmpeg", "https://ffmpeg.org/download.html");
  const snapshotsDir = join(cacheDir, CACHE_FILES.snapshotsDir);
  await mkdir(snapshotsDir, { recursive: true });

  const videoPath = join(cacheDir, CACHE_FILES.video);
  const frameMap = new Map<string, string>();
  const total = timestamps.length;
  const bar = new ProgressBar({ label: "Extracting frames", total });

  for (let i = 0; i < total; i++) {
    const ts = timestamps[i];
    const safeTs = ts.replace(/:/g, "");
    const filename = `snapshot-${safeTs}.jpg`;
    const outputPath = join(snapshotsDir, filename);
    const relativePath = `${CACHE_FILES.snapshotsDir}/${filename}`;

    bar.update(i + 1);

    // Skip if already extracted
    const file = Bun.file(outputPath);
    if (await file.exists() && file.size > 0) {
      if (verbose) console.error(`\n[extract-frames] ${ts} already cached`);
      frameMap.set(ts, relativePath);
      continue;
    }

    try {
      await run([
        ffmpeg,
        "-ss", ts,
        "-i", videoPath,
        "-vframes", "1",
        "-q:v", "2",
        "-y",
        outputPath,
      ]);
      frameMap.set(ts, relativePath);
    } catch (err) {
      if (verbose) {
        console.error(`\n[extract-frames] failed at ${ts}: ${err instanceof Error ? err.message : String(err)}`);
      }
    }
  }

  bar.done("done");
  return frameMap;
}
