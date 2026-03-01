import { Command } from "commander";
import { getProviders, mountProvider, unmountProvider } from "../api/client.js";

export function registerProviderCommands(program: Command): void {
    const providers = program.command("providers").description("Manage storage provider mounts");

    providers
        .command("list")
        .description("List all currently mounted providers")
        .action(async () => {
            try {
                const mounts = await getProviders();
                if (mounts.length === 0) {
                    console.log("No providers mounted.");
                    return;
                }
                for (const mount of mounts) {
                    const configStr = Object.entries(mount.config)
                        .map(([k, v]) => `${k}=${v}`)
                        .join(", ");
                    console.log(`  ${mount.mountId}  [${mount.scheme}]  ${configStr}`);
                }
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });

    providers
        .command("mount <mountId>")
        .description(
            "Register a storage provider under a named mount ID.\n" +
                "  <mountId>  A short name you choose (e.g. 'docs', 'backups') used to address\n" +
                "             files as <mountId>:/path in all other commands."
        )
        .requiredOption(
            "--scheme <scheme>",
            "Provider type to mount. Currently supported: local\n" + "             (local filesystem directory on the server machine)."
        )
        .option(
            "--config <pairs...>",
            "Provider configuration as key=value pairs.\n" +
                "             For --scheme local, required key:\n" +
                "               rootDir=<absolute-path>  The directory to expose via this mount."
        )
        .addHelpText(
            "after",
            "\nExample — mount /home/alice/documents as 'docs':\n" +
                "  $ files providers mount docs --scheme local --config rootDir=/home/alice/documents\n" +
                "\nThen access files with:\n" +
                "  $ files ls docs://\n" +
                "  $ files cat docs://report.pdf"
        )
        .action(async (mountId: string, options: { scheme: string; config?: string[] }) => {
            const config: Record<string, string> = {};
            for (const pair of options.config ?? []) {
                const eqIdx = pair.indexOf("=");
                if (eqIdx === -1) {
                    console.error(`Invalid config pair (expected key=value): ${pair}`);
                    process.exit(1);
                }
                const key = pair.slice(0, eqIdx);
                const value = pair.slice(eqIdx + 1);
                if (key) config[key] = value;
            }

            try {
                const result = await mountProvider(mountId, options.scheme, config);
                console.log(`Mounted '${result.mountId}' (${result.scheme})`);
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });

    providers
        .command("unmount <mountId>")
        .description("Unmount a storage provider")
        .action(async (mountId: string) => {
            try {
                await unmountProvider(mountId);
                console.log(`Unmounted '${mountId}'`);
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });
}
