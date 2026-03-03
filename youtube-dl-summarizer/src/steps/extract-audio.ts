import { join } from "node:path";
import { CACHE_FILES, cacheExists } from "../cache.ts";
import { ensureBinary, run } from "../utils/subprocess.ts";

export async function extractAudio(
  cacheDir: string,
  verbose: boolean,
): Promise<void> {
  if (await cacheExists(cacheDir, CACHE_FILES.audio)) {
    console.error("Extracting audio... cached, skipping");
    return;
  }

  const bin = ensureBinary("ffmpeg", "https://ffmpeg.org/download.html");
  console.error("Extracting audio...");

  await run([
    bin,
    "-i",
    join(cacheDir, CACHE_FILES.video),
    "-ar",
    "16000",
    "-ac",
    "1",
    "-y",
    join(cacheDir, CACHE_FILES.audio),
  ]);
}
