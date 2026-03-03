import {
  extractVideoId,
  getCacheDir,
  CACHE_FILES,
  cacheWrite,
  cacheExists,
  cacheRead,
} from "./cache.ts";
import { downloadVideo, downloadCaptions } from "./steps/download.ts";
import { extractAudio } from "./steps/extract-audio.ts";
import { transcribe } from "./steps/transcribe.ts";
import { summarize } from "./steps/summarize.ts";
import { parseCaptions } from "./utils/parse-captions.ts";

export interface PipelineOptions {
  prompt?: string;
  verbose?: boolean;
}

export async function runPipeline(
  url: string,
  opts: PipelineOptions = {},
): Promise<string> {
  const verbose = opts.verbose ?? false;

  // 1. Extract video ID and set up cache
  const videoId = extractVideoId(url);
  if (verbose) console.error(`[pipeline] video ID: ${videoId}`);

  const cacheDir = await getCacheDir(videoId);
  if (verbose) console.error(`[pipeline] cache dir: ${cacheDir}`);

  // 2. Download video
  await downloadVideo(url, cacheDir, verbose);

  // 3. Try to get captions
  const vttPath = await downloadCaptions(url, cacheDir, verbose);

  // 4. Build transcript — prefer captions, fall back to whisper
  if (vttPath && !(await cacheExists(cacheDir, CACHE_FILES.transcript))) {
    if (verbose) console.error("[pipeline] using official captions");
    const vttContent = await Bun.file(vttPath).text();
    const transcript = parseCaptions(vttContent);
    await cacheWrite(cacheDir, CACHE_FILES.transcript, transcript);
  } else if (!(await cacheExists(cacheDir, CACHE_FILES.transcript))) {
    if (verbose)
      console.error("[pipeline] no captions, using whisper.cpp");
    await extractAudio(cacheDir, verbose);
    await transcribe(cacheDir, verbose);
  }

  // 5. Summarize
  const summary = await summarize(cacheDir, verbose, opts.prompt);

  return summary;
}
