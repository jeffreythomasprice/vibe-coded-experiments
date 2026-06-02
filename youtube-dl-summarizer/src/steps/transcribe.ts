import { join } from "node:path";
import { CACHE_FILES, cacheExists } from "../cache.ts";
import { expandTilde } from "../utils/load-env.ts";
import { run } from "../utils/subprocess.ts";

function findWhisperBinary(): string {
  // Check env override first
  const envBin = process.env.WHISPER_BINARY;
  if (envBin) {
    const expanded = expandTilde(envBin);
    if (!Bun.which(expanded)) {
      throw new Error(`WHISPER_BINARY="${envBin}" not found on PATH`);
    }
    return expanded;
  }

  // Try common names (whisper-cli is the default whisper.cpp binary)
  for (const name of ["whisper-cli", "whisper-cpp", "whisper", "main"]) {
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
    console.error("Transcribing... cached, skipping");
    return;
  }

  const bin = findWhisperBinary();

  const modelEnv = process.env.WHISPER_MODEL;
  if (!modelEnv) {
    throw new Error(
      "WHISPER_MODEL env var is required (path to whisper.cpp model file, e.g. models/ggml-base.en.bin)",
    );
  }
  const model = expandTilde(modelEnv);

  console.error("Transcribing with whisper...");

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
