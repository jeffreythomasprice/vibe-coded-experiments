import pino from "pino";
import pinoPretty from "pino-pretty";
import { mkdirSync, createWriteStream } from "fs";
import { dirname, resolve } from "path";

const DEFAULT_LOG_FILE = "logs/discord-bot.log";

export let logger = pino(pinoPretty({ colorize: true }));

export function initLogger(logFile?: string) {
  const filePath = resolve(logFile ?? DEFAULT_LOG_FILE);
  mkdirSync(dirname(filePath), { recursive: true });

  const streams: pino.StreamEntry[] = [
    { level: "info", stream: pinoPretty({ colorize: true }) },
    { level: "debug", stream: createWriteStream(filePath, { flags: "a" }) },
  ];

  logger = pino({ level: "debug" }, pino.multistream(streams));
}
