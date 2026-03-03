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
import { parseCaptions, parseCaptionsWithTimestamps } from "./utils/parse-captions.ts";
import { extractFrames } from "./steps/extract-frames.ts";
import { enrichSummary } from "./steps/enrich-summary.ts";
import { generatePdf } from "./steps/generate-pdf.ts";

export interface PipelineOptions {
  prompt?: string;
  verbose?: boolean;
  snapshots?: boolean;
  pdf?: boolean;
}

export interface PipelineResult {
  summary: string;
  pdfPath?: string;
}

export async function runPipeline(
  url: string,
  opts: PipelineOptions = {},
): Promise<PipelineResult> {
  const verbose = opts.verbose ?? false;
  const snapshots = opts.snapshots ?? true;
  const pdf = opts.pdf ?? true;

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
    console.error("Building transcript from captions...");
    const vttContent = await Bun.file(vttPath).text();
    const transcript = parseCaptions(vttContent);
    await cacheWrite(cacheDir, CACHE_FILES.transcript, transcript);

    // Also write timestamped version for snapshot extraction
    if (!(await cacheExists(cacheDir, CACHE_FILES.timestampedTranscript))) {
      const timestamped = parseCaptionsWithTimestamps(vttContent);
      await cacheWrite(cacheDir, CACHE_FILES.timestampedTranscript, timestamped);
    }
  } else if (!(await cacheExists(cacheDir, CACHE_FILES.transcript))) {
    if (verbose)
      console.error("[pipeline] no captions, using whisper.cpp");
    await extractAudio(cacheDir, verbose);
    await transcribe(cacheDir, verbose);
  }

  // 5. Summarize
  const summary = await summarize(cacheDir, verbose, opts.prompt);

  // 6. Extract frames and enrich summary with snapshots
  let finalSummary = summary;
  if (snapshots) {
    const frameMap = await extractFrames(cacheDir, verbose);
    if (frameMap.size > 0) {
      finalSummary = await enrichSummary(cacheDir, frameMap, verbose);
    }
  }

  // 7. Generate PDF
  let pdfPath: string | undefined;
  if (pdf) {
    pdfPath = await generatePdf(cacheDir, verbose);
  }

  console.error("Done.");
  return { summary: finalSummary, pdfPath };
}
