import { z } from "zod/v4";

const configSchema = z.object({
  host: z.string().default("127.0.0.1"),
  port: z.int().default(45192),
  defaultRecipient: z.string().default("jeff0587"),
});

export type Config = z.infer<typeof configSchema>;

export async function loadConfig(path?: string): Promise<Config> {
  const configPath = path ?? "./discord-bot.config.json";
  const file = Bun.file(configPath);

  if (!(await file.exists())) {
    throw new Error(`Config file not found: ${configPath}`);
  }

  const raw = await file.json();
  return configSchema.parse(raw);
}
