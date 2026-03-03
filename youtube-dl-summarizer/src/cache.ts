import { mkdir } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

const CACHE_BASE = join(homedir(), ".cache", "yt-summarizer");

export const CACHE_FILES = {
  video: "video.mp4",
  captions: "captions.vtt",
  audio: "audio.wav",
  transcript: "transcript.txt",
  timestampedTranscript: "transcript-timestamped.txt",
  summary: "summary.md",
  snapshotsDir: "snapshots",
  summaryWithSnapshots: "summary-with-snapshots.md",
  summaryPdf: "summary.pdf",
} as const;

/**
 * Extract YouTube video ID from various URL formats.
 */
export function extractVideoId(url: string): string {
  // Try standard URL parsing first
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new Error(`Invalid URL: ${url}`);
  }

  const hostname = parsed.hostname.replace("www.", "");

  // youtube.com/watch?v=ID
  if (
    (hostname === "youtube.com" || hostname === "m.youtube.com") &&
    parsed.pathname === "/watch"
  ) {
    const id = parsed.searchParams.get("v");
    if (id) return id;
  }

  // youtu.be/ID
  if (hostname === "youtu.be") {
    const id = parsed.pathname.slice(1).split("/")[0];
    if (id) return id;
  }

  // youtube.com/embed/ID or youtube.com/shorts/ID or youtube.com/v/ID
  const embedMatch = parsed.pathname.match(
    /^\/(embed|shorts|v)\/([^/?]+)/,
  );
  if (embedMatch?.[2]) return embedMatch[2];

  throw new Error(`Could not extract video ID from: ${url}`);
}

/**
 * Get cache directory for a video, creating it if needed.
 */
export async function getCacheDir(videoId: string): Promise<string> {
  const dir = join(CACHE_BASE, videoId);
  await mkdir(dir, { recursive: true });
  return dir;
}

/**
 * Check if a cached file exists and has content.
 */
export async function cacheExists(
  cacheDir: string,
  filename: string,
): Promise<boolean> {
  const file = Bun.file(join(cacheDir, filename));
  return (await file.exists()) && file.size > 0;
}

/**
 * Read a cached file.
 */
export async function cacheRead(
  cacheDir: string,
  filename: string,
): Promise<string> {
  return Bun.file(join(cacheDir, filename)).text();
}

/**
 * Write content to a cache file.
 */
export async function cacheWrite(
  cacheDir: string,
  filename: string,
  content: string,
): Promise<void> {
  await Bun.write(join(cacheDir, filename), content);
}
