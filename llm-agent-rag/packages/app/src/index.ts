#!/usr/bin/env bun
import { Command } from "commander";
import chalk from "chalk";
import Table from "cli-table3";
import { initConfig } from "./config.js";
import { closeSql } from "./db.js";
import { print, printError, setSilent } from "./output.js";

function parseTags(tagStrings: string[]): string[] {
  return tagStrings.map((t) => t.trim()).filter(Boolean);
}

const program = new Command();

program
  .name("rag")
  .description("Local RAG system powered by Ollama + pgvector")
  .hook("preAction", async () => {
    await initConfig();
  });

// -- Ingest --

program
  .command("ingest")
  .description("Ingest a file (txt or pdf) into the vector store")
  .argument("<filepath>", "Path to file")
  .option("-t, --tag <tags...>", "Tag string (repeatable)")
  .action(async (filepath: string, opts: { tag?: string[] }) => {
    const { ingestFile } = await import("./ingest.js");
    const tags = parseTags(opts.tag ?? []);
    const result = await ingestFile(filepath, tags);
    print(JSON.stringify(result, null, 2));
    await closeSql();
  });

program
  .command("ingest-dir")
  .description("Ingest all matching files in a directory")
  .argument("<directory>", "Directory path")
  .option("-t, --tag <tags...>", "Tag string (repeatable)")
  .option("--ext <extensions...>", "File extensions to include", [".txt", ".pdf"])
  .action(
    async (
      directory: string,
      opts: { tag?: string[]; ext: string[] },
    ) => {
      const { readdir } = await import("fs/promises");
      const { join, extname, basename } = await import("path");
      const { ingestFile } = await import("./ingest.js");

      const tags = parseTags(opts.tag ?? []);
      const exts = new Set(opts.ext.map((e) => (e.startsWith(".") ? e : `.${e}`)));

      // Recursively find files
      async function findFiles(dir: string): Promise<string[]> {
        const entries = await readdir(dir, { withFileTypes: true });
        const files: string[] = [];
        for (const entry of entries) {
          const fullPath = join(dir, entry.name);
          if (entry.isDirectory()) {
            files.push(...(await findFiles(fullPath)));
          } else if (exts.has(extname(entry.name).toLowerCase())) {
            files.push(fullPath);
          }
        }
        return files;
      }

      const files = await findFiles(directory);
      if (files.length === 0) {
        print(
          chalk.yellow(
            `No files matching ${[...exts].join(", ")} found in ${directory}`,
          ),
        );
        await closeSql();
        return;
      }

      print(`Found ${files.length} file(s) to ingest`);
      for (const f of files) {
        await ingestFile(f, tags);
      }
      await closeSql();
    },
  );

// -- Query --

program
  .command("query")
  .description("Query the vector store with natural language")
  .argument("<query>", "Search query")
  .option("-t, --tag <tags...>", "Filter by tag")
  .option("-k, --top-k <number>", "Number of results", "5")
  .option("--raw", "Return raw chunks without LLM synthesis")
  .action(
    async (
      query: string,
      opts: { tag?: string[]; topK: string; raw?: boolean },
    ) => {
      const tagList =
        opts.tag && opts.tag.length > 0 ? parseTags(opts.tag) : undefined;
      const topK = parseInt(opts.topK, 10);

      if (opts.raw) {
        const { retrieve } = await import("./query.js");
        const results = await retrieve(query, topK, tagList);
        printResultsTable(results);
      } else {
        const { ask } = await import("./query.js");
        const result = await ask(query, topK, tagList);
        print(`\n${chalk.bold("Answer:")}\n${result.answer}\n`);
        if (result.sources.length > 0) {
          print(chalk.dim("Sources:"));
          for (const s of result.sources) {
            print(
              `  - ${s.document} (chunk ${s.chunk_index}, similarity ${(s.similarity as number).toFixed(3)})`,
            );
          }
        }
      }
      await closeSql();
    },
  );

// -- Agent --

program
  .command("agent")
  .description("Ask the agentic assistant (LLM decides when/how to search)")
  .argument("<message>", "Your question")
  .action(async (message: string) => {
    const { agentChat } = await import("./agent.js");
    const answer = await agentChat(message);
    print(`\n${chalk.bold("Agent:")}\n${answer}`);
    await closeSql();
  });

// -- Browse & filter --

program
  .command("documents")
  .description("List all ingested documents with their tags")
  .action(async () => {
    const { listDocuments, closeSql: close } = await import("./db.js");
    const docs = await listDocuments();

    if (docs.length === 0) {
      print(chalk.yellow("No documents found."));
      await close();
      return;
    }

    const table = new Table({
      head: ["ID", "Name", "Ingested At", "Tags"],
    });
    for (const d of docs) {
      table.push([String(d.id), d.name, d.ingested_at, d.tags.join(", ")]);
    }
    print(table.toString());
    await close();
  });

program
  .command("tags")
  .description("List all tags and how many documents use each one")
  .action(async () => {
    const { listTags, closeSql: close } = await import("./db.js");
    const tagList = await listTags();

    if (tagList.length === 0) {
      print(chalk.yellow("No tags found."));
      await close();
      return;
    }

    const table = new Table({
      head: ["Tag", "Documents"],
    });
    for (const t of tagList) {
      table.push([t.tag, String(t.doc_count)]);
    }
    print(table.toString());
    await close();
  });

program
  .command("find")
  .description("Find documents matching all given tags (AND logic)")
  .requiredOption("-t, --tag <tags...>", "Filter by tag (repeatable)")
  .action(async (opts: { tag: string[] }) => {
    const { findDocumentsByTags, closeSql: close } = await import("./db.js");
    const tags = parseTags(opts.tag);
    const docs = await findDocumentsByTags(tags);

    if (docs.length === 0) {
      print(chalk.yellow("No documents match the given tags."));
      await close();
      return;
    }

    const table = new Table({
      head: ["ID", "Name", "Ingested At"],
    });
    for (const d of docs) {
      table.push([String(d.id), d.name, d.ingested_at]);
    }
    print(table.toString());
    await close();
  });

// -- Server --

program
  .command("serve")
  .description("Start the HTTP API server")
  .action(async () => {
    setSilent(true);
    const { startServer } = await import("./server.js");
    await startServer();
  });

// -- Utilities --

function printResultsTable(
  results: { document_name: string; chunk_index: number; similarity: number; content: string }[],
) {
  const table = new Table({
    head: ["#", "Document", "Chunk", "Similarity", "Content"],
    colWidths: [4, 20, 7, 12, 80],
  });
  for (let i = 0; i < results.length; i++) {
    const r = results[i];
    const excerpt = r.content.slice(0, 150).replace(/\n/g, " ");
    table.push([
      String(i + 1),
      r.document_name,
      String(r.chunk_index),
      r.similarity.toFixed(4),
      excerpt + "...",
    ]);
  }
  print(table.toString());
}

program.parse();
