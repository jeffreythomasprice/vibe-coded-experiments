#!/usr/bin/env bun
import { Command } from "commander";
import { runPipeline } from "./pipeline.ts";

const program = new Command();

program
  .name("yt-summarize")
  .description("Download a YouTube video, transcribe it, and summarize with Claude")
  .version("0.1.0");

program
  .command("summarize <url>", { isDefault: true })
  .description("Summarize a YouTube video")
  .option("-p, --prompt <prompt>", "Custom summarization prompt")
  .option("-v, --verbose", "Show progress details on stderr")
  .action(async (url: string, opts: { prompt?: string; verbose?: boolean }) => {
    try {
      const summary = await runPipeline(url, {
        prompt: opts.prompt,
        verbose: opts.verbose,
      });
      console.log(summary);
    } catch (err) {
      console.error(
        `Error: ${err instanceof Error ? err.message : String(err)}`,
      );
      process.exit(1);
    }
  });

program.parse();
