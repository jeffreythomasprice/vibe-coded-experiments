#!/usr/bin/env bun
import { Command } from "commander";
import { registerProviderCommands } from "./commands/providers.js";
import { registerFileCommands } from "./commands/files.js";
import { registerConfigCommands } from "./commands/config.js";

const program = new Command();

program.name("files").description("File manager CLI — manage files across local and remote storage").version("0.1.0");

registerProviderCommands(program);
registerFileCommands(program);
registerConfigCommands(program);

await program.parseAsync(process.argv);
