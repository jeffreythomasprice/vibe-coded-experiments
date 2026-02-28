import { Command } from "commander";
import { parseUri, listFiles, statFile, readFile, writeFile, deleteFile, moveFile, copyFile, mkdirFile } from "../api/client.js";

export function registerFileCommands(program: Command): void {
    // files ls <uri>
    program
        .command("ls <uri>")
        .description("List directory contents")
        .action(async (uri: string) => {
            try {
                const { mountId, path } = parseUri(uri);
                const entries = await listFiles(mountId, path);
                if (entries.length === 0) {
                    console.log("(empty)");
                    return;
                }
                for (const entry of entries) {
                    const typeChar = entry.type === "directory" ? "d" : "-";
                    const sizeStr = entry.type === "directory" ? "       -" : String(entry.size).padStart(8);
                    const modStr = new Date(entry.modifiedAt).toISOString().slice(0, 16).replace("T", " ");
                    console.log(`${typeChar}  ${sizeStr}  ${modStr}  ${entry.name}`);
                }
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });

    // files stat <uri>
    program
        .command("stat <uri>")
        .description("Show file/directory metadata")
        .action(async (uri: string) => {
            try {
                const { mountId, path } = parseUri(uri);
                const stat = await statFile(mountId, path);
                console.log(`Name:       ${stat.name}`);
                console.log(`Path:       ${stat.path}`);
                console.log(`Type:       ${stat.type}`);
                console.log(`Size:       ${stat.size} bytes`);
                console.log(`Created:    ${new Date(stat.createdAt).toISOString()}`);
                console.log(`Modified:   ${new Date(stat.modifiedAt).toISOString()}`);
                if (stat.mimeType) {
                    console.log(`MIME:       ${stat.mimeType}`);
                }
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });

    // files cat <uri>
    program
        .command("cat <uri>")
        .description("Stream file contents to stdout")
        .action(async (uri: string) => {
            try {
                const { mountId, path } = parseUri(uri);
                const stream = await readFile(mountId, path);
                const reader = stream.getReader();
                const stdout = process.stdout;

                while (true) {
                    const { done, value } = await reader.read();
                    if (done) break;
                    stdout.write(value);
                }
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });

    // files cp <src> <dest>
    program
        .command("cp <src> <dest>")
        .description("Copy a file or directory. src may be a local path or a URI; dest must be a URI.")
        .action(async (src: string, dest: string) => {
            try {
                if (!src.includes("://")) {
                    // Local file → remote URI upload
                    const { mountId, path } = parseUri(dest);
                    const file = Bun.file(src);
                    await writeFile(mountId, path, file.stream());
                } else {
                    await copyFile(src, dest);
                }
                console.log(`Copied ${src} → ${dest}`);
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });

    // files mv <src> <dest>
    program
        .command("mv <src> <dest>")
        .description("Move/rename a file")
        .action(async (src: string, dest: string) => {
            try {
                await moveFile(src, dest);
                console.log(`Moved ${src} → ${dest}`);
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });

    // files mkdir <uri>
    program
        .command("mkdir <uri>")
        .description("Create a directory")
        .action(async (uri: string) => {
            try {
                const { mountId, path } = parseUri(uri);
                await mkdirFile(mountId, path);
                console.log(`Created ${uri}`);
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });

    // files rm <uri>
    program
        .command("rm <uri>")
        .description("Delete a file or directory")
        .action(async (uri: string) => {
            try {
                const { mountId, path } = parseUri(uri);
                await deleteFile(mountId, path);
                console.log(`Deleted ${uri}`);
            } catch (err) {
                console.error("Error:", (err as Error).message);
                process.exit(1);
            }
        });
}
