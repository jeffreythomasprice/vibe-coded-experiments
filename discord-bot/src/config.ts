import { z } from "zod/v4";
import { dirname, resolve, parse } from "path";

const CONFIG_FILENAME = "discord-bot.config.json";

const configSchema = z.object({
  host: z.string().default("127.0.0.1"),
  port: z.int().default(45192),
  defaultRecipient: z.string().default("jeff0587"),
  logFile: z.string().optional(),
});

export type Config = z.infer<typeof configSchema>;

async function findConfig(): Promise<string | null> {
  const candidates: string[] = [];

  // CWD first
  candidates.push(resolve(process.cwd(), CONFIG_FILENAME));

  // Binary dir, then every parent up to root
  let dir = dirname(process.execPath);
  while (true) {
    candidates.push(resolve(dir, CONFIG_FILENAME));
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }

  for (const candidate of candidates) {
    if (await Bun.file(candidate).exists()) return candidate;
  }
  return null;
}

export async function loadConfig(path?: string): Promise<Config> {
  const configPath = path ?? (await findConfig());

  if (!configPath) {
    throw new Error(
      `Config file not found: searched CWD and ancestors of ${dirname(process.execPath)}`,
    );
  }

  const raw = await Bun.file(configPath).json();
  return configSchema.parse(raw);
}
