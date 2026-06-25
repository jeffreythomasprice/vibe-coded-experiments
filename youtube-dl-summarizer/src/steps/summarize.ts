import Anthropic from "@anthropic-ai/sdk";
import { CACHE_FILES, cacheExists, cacheRead, cacheWrite } from "../cache.ts";

const DEFAULT_PROMPT = `You are summarizing a YouTube video transcript that includes [HH:MM:SS] timestamps. Provide:

1. A concise summary (2-3 paragraphs)
2. Key points as bullet points — include the [HH:MM:SS] timestamp at the start of each bullet where the topic is discussed
3. Any notable quotes — include the [HH:MM:SS] timestamp before each quote

Use the exact [HH:MM:SS] format from the transcript for timestamps. Be concise and informative. Focus on the main ideas and takeaways.`;

export async function summarize(
  cacheDir: string,
  verbose: boolean,
  customPrompt?: string,
): Promise<string> {
  if (await cacheExists(cacheDir, CACHE_FILES.summary)) {
    console.error("Summarizing... cached, skipping");
    return cacheRead(cacheDir, CACHE_FILES.summary);
  }

  // Prefer timestamped transcript when available
  const transcriptFile = (await cacheExists(cacheDir, CACHE_FILES.timestampedTranscript))
    ? CACHE_FILES.timestampedTranscript
    : CACHE_FILES.transcript;
  const transcript = await cacheRead(cacheDir, transcriptFile);
  const prompt = customPrompt || DEFAULT_PROMPT;

  console.error("Summarizing with Claude...");

  // Construct lazily so loadEnv() has already populated ANTHROPIC_API_KEY.
  const client = new Anthropic();
  const message = await client.messages.create({
    model: "claude-sonnet-4-6",
    max_tokens: 4096,
    system: prompt,
    messages: [
      { role: "user", content: `Transcript:\n\n${transcript}` },
    ],
  });

  const summary = message.content
    .filter((block) => block.type === "text")
    .map((block) => block.text)
    .join("\n")
    .trim();

  await cacheWrite(cacheDir, CACHE_FILES.summary, summary);
  return summary;
}
