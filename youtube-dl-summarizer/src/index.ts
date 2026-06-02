#!/usr/bin/env bun
import { Command } from "commander";
import { runPipeline } from "./pipeline.ts";
import { loadEnv } from "./utils/load-env.ts";

await loadEnv();

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
  .option("--no-snapshots", "Disable video frame extraction")
  .option("--no-pdf", "Skip PDF generation")
  .action(async (url: string, opts: { prompt?: string; verbose?: boolean; snapshots?: boolean; pdf?: boolean }) => {
    try {
      const { summary, pdfPath } = await runPipeline(url, {
        prompt: opts.prompt,
        verbose: opts.verbose,
        snapshots: opts.snapshots,
        pdf: opts.pdf,
      });
      console.log(summary);
      if (pdfPath) {
        console.error(`PDF saved to: ${pdfPath}`);
      }
    } catch (err) {
      console.error(
        `Error: ${err instanceof Error ? err.message : String(err)}`,
      );
      process.exit(1);
    }
  });

program.parse();
