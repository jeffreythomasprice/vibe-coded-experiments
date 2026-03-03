import { CACHE_FILES, cacheExists, cacheRead, cacheWrite } from "../cache.ts";
import { ensureBinary, run } from "../utils/subprocess.ts";

const DEFAULT_PROMPT = `You are summarizing a YouTube video transcript. Provide:

1. A concise summary (2-3 paragraphs)
2. Key points as bullet points
3. Any notable quotes

Be concise and informative. Focus on the main ideas and takeaways.`;

export async function summarize(
  cacheDir: string,
  verbose: boolean,
  customPrompt?: string,
): Promise<string> {
  if (await cacheExists(cacheDir, CACHE_FILES.summary)) {
    if (verbose) console.error("[summarize] already cached, skipping");
    return cacheRead(cacheDir, CACHE_FILES.summary);
  }

  const bin = ensureBinary(
    "claude",
    "https://docs.anthropic.com/en/docs/claude-code",
  );

  const transcript = await cacheRead(cacheDir, CACHE_FILES.transcript);
  const prompt = customPrompt || DEFAULT_PROMPT;
  const fullPrompt = `${prompt}\n\n---\n\nTranscript:\n\n${transcript}`;

  if (verbose) console.error("[summarize] generating summary with Claude...");

  const result = await run([
    bin,
    "-p",
    fullPrompt,
    "--output-format",
    "text",
  ]);

  const summary = result.stdout.trim();
  await cacheWrite(cacheDir, CACHE_FILES.summary, summary);
  return summary;
}
