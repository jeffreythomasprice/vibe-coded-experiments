import { env } from "bun";

const REQUIRED_ENV_VARS = [
  "OLLAMA_BASE_URL",
  "EMBED_MODEL",
  "CHAT_MODEL",
  "DB_DSN",
  "CHUNK_SIZE",
  "CHUNK_OVERLAP",
  "CONTEXT_WINDOW",
] as const;

export let OLLAMA_BASE_URL: string;
export let EMBED_MODEL: string;
export let CHAT_MODEL: string;
export let DB_DSN: string;
export let CHUNK_SIZE: number;
export let CHUNK_OVERLAP: number;
export let CONTEXT_WINDOW: number;

async function ensureOllamaModel(model: string): Promise<void> {
  let ollamaName: string;
  try {
    const resp = await fetch(`${OLLAMA_BASE_URL}/api/tags`, {
      signal: AbortSignal.timeout(10_000),
    });
    if (!resp.ok) {
      console.warn(`Warning: could not check ollama models (HTTP ${resp.status})`);
      return;
    }
    const data = (await resp.json()) as { models?: { name: string }[] };
    const localModels = (data.models ?? []).map((m) => m.name);

    ollamaName = model;
    const found = localModels.some(
      (m) =>
        m === ollamaName ||
        m === `${ollamaName}:latest` ||
        m.split(":")[0] === ollamaName,
    );
    if (found) return;
  } catch (err) {
    console.warn(`Warning: could not check ollama models (${err})`);
    return;
  }

  console.log(`Model '${ollamaName}' not found locally — pulling from ollama...`);
  const resp = await fetch(`${OLLAMA_BASE_URL}/api/pull`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name: ollamaName }),
  });

  if (!resp.ok || !resp.body) {
    console.error(`Error: failed to pull model '${ollamaName}' (HTTP ${resp.status})`);
    process.exit(1);
  }

  const reader = resp.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    const lines = buf.split("\n");
    buf = lines.pop()!;
    for (const line of lines) {
      if (!line.trim()) continue;
      const data = JSON.parse(line) as {
        status?: string;
        total?: number;
        completed?: number;
      };
      const status = data.status ?? "";
      if (/pulling|verifying|writing/.test(status)) {
        if (data.total) {
          const pct = ((data.completed ?? 0) / data.total) * 100;
          process.stdout.write(`\r  ${status}: ${pct.toFixed(0)}%`);
        } else {
          process.stdout.write(`\r  ${status}...`);
        }
      } else if (status === "success") {
        console.log(`\n  Model '${ollamaName}' pulled successfully.`);
      }
    }
  }
}

export async function initConfig(): Promise<void> {
  const missing = REQUIRED_ENV_VARS.filter((v) => !env[v]);
  if (missing.length > 0) {
    console.error(
      `Error: missing required environment variables: ${missing.join(", ")}\n` +
        "Set them in your .env file or environment before running.",
    );
    process.exit(1);
  }

  OLLAMA_BASE_URL = env.OLLAMA_BASE_URL!;
  EMBED_MODEL = env.EMBED_MODEL!;
  CHAT_MODEL = env.CHAT_MODEL!;
  DB_DSN = env.DB_DSN!;
  CHUNK_SIZE = parseInt(env.CHUNK_SIZE!, 10);
  CHUNK_OVERLAP = parseInt(env.CHUNK_OVERLAP!, 10);
  CONTEXT_WINDOW = parseInt(env.CONTEXT_WINDOW!, 10);

  await ensureOllamaModel(EMBED_MODEL);
  await ensureOllamaModel(CHAT_MODEL);
}
