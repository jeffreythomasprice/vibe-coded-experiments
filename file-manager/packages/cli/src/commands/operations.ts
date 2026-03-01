import { Command } from "commander";
import { getOperations } from "../api/client.js";

export function registerOperationCommands(program: Command): void {
    program
        .command("ops")
        .description("List pending/active file operations")
        .option("-s, --status <status>", "Filter by status (pending, in_progress, completed, failed)")
        .action(async (opts: { status?: string }) => {
            try {
                const operations = await getOperations(opts.status);
                if (operations.length === 0) {
                    console.log("No operations.");
                    return;
                }

                for (const op of operations) {
                    const percent = op.totalFiles > 0
                        ? `${Math.round((op.processedFiles / op.totalFiles) * 100)}%`
                        : "-";
                    const status = op.status.padEnd(11);
                    const type = op.type.padEnd(4);
                    const progress = op.status === "in_progress"
                        ? `${percent} (${op.processedFiles}/${op.totalFiles} files)`
                        : op.status === "completed" ? "100%" : op.error ?? "";
                    console.log(`${op.id.slice(0, 8)}  ${type}  ${status}  ${op.src} → ${op.dest}  ${progress}`);
                }
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });
}
