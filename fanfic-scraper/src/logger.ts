import { mkdirSync, appendFileSync } from "node:fs";
import { join } from "node:path";

export type LogLevel = "debug" | "info" | "warn" | "error";

interface LogEntry {
  timestamp: string;
  level: LogLevel;
  message: string;
  [key: string]: unknown;
}

const LEVEL_ORDER: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

export interface LoggerOptions {
  dir: string;
  verbose?: boolean;
  level?: LogLevel;
}

export class Logger {
  private logFile: string;
  private verbose: boolean;
  private minLevel: number;

  constructor(options: LoggerOptions) {
    mkdirSync(options.dir, { recursive: true });
    const date = new Date().toISOString().slice(0, 10);
    this.logFile = join(options.dir, `fanfic-scraper-${date}.log`);
    this.verbose = options.verbose ?? false;
    this.minLevel = LEVEL_ORDER[options.level ?? "debug"];
  }

  private write(level: LogLevel, message: string, meta?: Record<string, unknown>): void {
    if (LEVEL_ORDER[level] < this.minLevel) return;

    const entry: LogEntry = {
      timestamp: new Date().toISOString(),
      level,
      message,
      ...meta,
    };

    const line = JSON.stringify(entry);
    appendFileSync(this.logFile, line + "\n");

    if (this.verbose) {
      console.error(this.formatForConsole(entry));
    }
  }

  private formatForConsole(entry: LogEntry): string {
    const time = entry.timestamp.slice(11, 23); // HH:mm:ss.SSS
    const lvl = entry.level.toUpperCase().padEnd(5);
    const { timestamp: _, level: __, message, ...rest } = entry;
    const meta = Object.keys(rest).length > 0 ? ` ${JSON.stringify(rest)}` : "";
    return `${time} ${lvl} ${message}${meta}`;
  }

  debug(message: string, meta?: Record<string, unknown>): void {
    this.write("debug", message, meta);
  }

  info(message: string, meta?: Record<string, unknown>): void {
    this.write("info", message, meta);
  }

  warn(message: string, meta?: Record<string, unknown>): void {
    this.write("warn", message, meta);
  }

  error(message: string, meta?: Record<string, unknown>): void {
    this.write("error", message, meta);
  }
}

/** No-op logger that discards all messages. */
export class NullLogger extends Logger {
  constructor() {
    super({ dir: "/tmp/fanfic-scraper/logs" });
  }
  override debug(): void {}
  override info(): void {}
  override warn(): void {}
  override error(): void {}
}
