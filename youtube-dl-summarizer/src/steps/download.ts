import { join } from "node:path";
import { CACHE_FILES, cacheExists } from "../cache.ts";
import { ensureBinary, run } from "../utils/subprocess.ts";
import { ProgressBar } from "../utils/progress.ts";
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
  const bar = new ProgressBar({ label: "Downloading video" });
  bar.updatePercent(0);

  // yt-dlp outputs progress on stderr with \r separators
  let stderrBuf = "";
  // yt-dlp writes progress to stdout
  let buf = "";
  const onStdout = (chunk: string) => {
    buf += chunk;
    const lines = buf.split(/[\r\n]/);
    buf = lines.pop() ?? "";
    for (const line of lines) {
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
      join(cacheDir, CACHE_FILES.video),
      "--no-playlist",
      "--newline",
      url,
    ],
    { onStdout },
  );

  bar.done("done");
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
