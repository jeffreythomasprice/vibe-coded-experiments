import { join } from "node:path";
import { CACHE_FILES, cacheExists } from "../cache.ts";
import { ensureBinary, run } from "../utils/subprocess.ts";
import { Glob } from "bun";

const INSTALL_HINT = "https://github.com/yt-dlp/yt-dlp#installation";

export async function downloadVideo(
  url: string,
  cacheDir: string,
  verbose: boolean,
): Promise<void> {
  if (await cacheExists(cacheDir, CACHE_FILES.video)) {
    console.error("Downloading video... cached, skipping");
    return;
  }

  const bin = ensureBinary("yt-dlp", INSTALL_HINT);
  console.error("Downloading video...");

  await run(
    [
      bin,
      "-f",
      "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best",
      "-o",
      join(cacheDir, CACHE_FILES.video),
      "--no-playlist",
      url,
    ],
    { streamStderr: true },
  );
}

export async function downloadCaptions(
  url: string,
  cacheDir: string,
  verbose: boolean,
): Promise<string | null> {
  // Check if we already have a .vtt file
  const existing = await findVttFile(cacheDir);
  if (existing) {
    console.error("Downloading captions... cached, skipping");
    return existing;
  }

  const bin = ensureBinary("yt-dlp", INSTALL_HINT);
  console.error("Downloading captions...");

  try {
    await run([
      bin,
      "--write-sub",
      "--write-auto-sub",
      "--sub-lang",
      "en",
      "--sub-format",
      "vtt",
      "--skip-download",
      "-o",
      join(cacheDir, "captions"),
      url,
    ]);
  } catch {
    if (verbose) console.error("[download] no captions available");
    return null;
  }

  return findVttFile(cacheDir);
}

async function findVttFile(cacheDir: string): Promise<string | null> {
  const glob = new Glob("*.vtt");
  for await (const file of glob.scan(cacheDir)) {
    return join(cacheDir, file);
  }
  return null;
}
