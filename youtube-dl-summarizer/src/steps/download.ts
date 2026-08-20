import { join } from "node:path";
import { CACHE_FILES, cacheExists } from "../cache.ts";
import { ensureBinary, run } from "../utils/subprocess.ts";
import { ProgressBar } from "../utils/progress.ts";
import { Glob } from "bun";

const INSTALL_HINT = "https://github.com/yt-dlp/yt-dlp#installation";

// YouTube blocks yt-dlp's default player client with 403s; forcing these
// clients works around it.
const EXTRACTOR_ARGS = [
  "--extractor-args",
  "youtube:player-client=web_embedded,web,tv",
];

export async function downloadVideo(
  url: string,
  cacheDir: string,
  verbose: boolean,
): Promise<void> {
  const videoPath = join(cacheDir, CACHE_FILES.video);

  if (await cacheExists(cacheDir, CACHE_FILES.video)) {
    console.error(`Downloading video... cached, skipping (${videoPath})`);
    return;
  }

  const bin = ensureBinary("yt-dlp", INSTALL_HINT);
  let bar = new ProgressBar({ label: "Downloading video" });
  bar.updatePercent(0);

  // yt-dlp writes progress to stdout
  let buf = "";
  let destinationCount = 0;
  const onStdout = (chunk: string) => {
    buf += chunk;
    const lines = buf.split(/[\r\n]/);
    buf = lines.pop() ?? "";
    for (const line of lines) {
      if (/^\[download\] Destination:/.test(line)) {
        destinationCount++;
        // yt-dlp downloads video and audio as separate streams when muxing.
        // On the second Destination line, finalize the first bar and start a new one.
        if (destinationCount === 2) {
          bar.done("done");
          bar = new ProgressBar({ label: "Downloading audio" });
          bar.updatePercent(0);
        }
        continue;
      }
      const match = line.match(/\[download\]\s+([\d.]+)%/);
      if (match) {
        const percent = parseFloat(match[1]);
        const extra = line.match(/at\s+(\S+)\s+ETA\s+(\S+)/);
        const suffix = extra ? `${extra[1]}  ETA ${extra[2]}` : undefined;
        bar.updatePercent(percent, suffix);
      }
    }
  };

  await run(
    [
      bin,
      "-f",
      "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best",
      "-o",
      videoPath,
      "--no-playlist",
      "--newline",
      ...EXTRACTOR_ARGS,
      url,
    ],
    { onStdout },
  );

  bar.done("done");
  console.error(`Video saved to ${videoPath}`);
}

export async function downloadCaptions(
  url: string,
  cacheDir: string,
  verbose: boolean,
): Promise<string | null> {
  // Check if we already have a .vtt file
  const existing = await findVttFile(cacheDir);
  if (existing) {
    console.error(`Downloading captions... cached, skipping (${existing})`);
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
      ...EXTRACTOR_ARGS,
      url,
    ]);
  } catch {
    if (verbose) console.error("[download] no captions available");
    return null;
  }

  const vttPath = await findVttFile(cacheDir);
  if (vttPath) console.error(`Captions saved to ${vttPath}`);
  return vttPath;
}

async function findVttFile(cacheDir: string): Promise<string | null> {
  const glob = new Glob("*.vtt");
  for await (const file of glob.scan(cacheDir)) {
    return join(cacheDir, file);
  }
  return null;
}
