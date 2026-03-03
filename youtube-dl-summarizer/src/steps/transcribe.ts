import { join } from "node:path";
import { CACHE_FILES, cacheExists } from "../cache.ts";
import { ensureBinary, run } from "../utils/subprocess.ts";

function findWhisperBinary(): string {
  // Check env override first
  const envBin = process.env.WHISPER_BINARY;
  if (envBin) {
    if (!Bun.which(envBin)) {
      throw new Error(`WHISPER_BINARY="${envBin}" not found on PATH`);
    }
    return envBin;
  }

  // Try common names
  for (const name of ["whisper-cpp", "whisper", "main"]) {
    const path = Bun.which(name);
    if (path) return path;
  }

  throw new Error(
    '"whisper.cpp" is not installed or not on your PATH.\n' +
      "Install: https://github.com/ggerganov/whisper.cpp\n" +
      "Or set WHISPER_BINARY env var to the binary path.",
  );
}

export async function transcribe(
  cacheDir: string,
  verbose: boolean,
): Promise<void> {
  if (await cacheExists(cacheDir, CACHE_FILES.transcript)) {
    if (verbose) console.error("[transcribe] already cached, skipping");
    return;
  }

  const bin = findWhisperBinary();

  const model = process.env.WHISPER_MODEL;
  if (!model) {
    throw new Error(
      "WHISPER_MODEL env var is required (path to whisper.cpp model file, e.g. models/ggml-base.en.bin)",
    );
  }

  if (verbose) console.error("[transcribe] transcribing audio...");

  // whisper.cpp outputs transcript.txt when --output-file is set (appends .txt)
  const outputBase = join(cacheDir, "transcript");

  await run([
    bin,
    "-m",
    model,
    "-f",
    join(cacheDir, CACHE_FILES.audio),
    "--output-txt",
    "--output-file",
    outputBase,
  ]);
}
