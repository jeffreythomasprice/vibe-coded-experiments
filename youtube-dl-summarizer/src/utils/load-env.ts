/**
 * .env loading with multi-directory resolution and tilde expansion.
 *
 * Bun auto-loads .env from the current working directory only. To support
 * running a compiled binary from anywhere while still picking up the .env that
 * lives beside it (or in the project dir), we walk up the directory tree from
 * both the CWD and the binary's location.
 */

import { homedir } from "node:os";
import { dirname, join } from "node:path";

/** Expand a leading `~` to the user's home directory. */
export function expandTilde(p: string): string {
  if (p === "~") return homedir();
  if (p.startsWith("~/")) return join(homedir(), p.slice(2));
  return p;
}

/** Directories to search for .env, in priority order (first match wins). */
function candidateDirs(): string[] {
  const dirs: string[] = [];
  const seen = new Set<string>();

  const addChain = (start: string) => {
    let dir = start;
    while (true) {
      if (!seen.has(dir)) {
        seen.add(dir);
        dirs.push(dir);
      }
      const parent = dirname(dir);
      if (parent === dir) break; // reached filesystem root
      dir = parent;
    }
  };

  // CWD chain first so a local .env overrides one beside the binary.
  addChain(process.cwd());
  // Then the binary's directory chain. For a compiled standalone executable
  // process.execPath is the binary itself; in dev it's the `bun` binary.
  addChain(dirname(process.execPath));

  return dirs;
}

/** Minimal dotenv parser: KEY=VALUE per line, `#` comments, optional quotes. */
function parseEnv(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq === -1) continue;
    const key = line.slice(0, eq).trim();
    if (!key) continue;
    let value = line.slice(eq + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    out[key] = value;
  }
  return out;
}

/**
 * Load the first `.env` found by walking up from the CWD and the binary dir.
 * Existing environment variables are never overridden, so real shell env and
 * Bun's own auto-load take precedence.
 */
export async function loadEnv(): Promise<void> {
  for (const dir of candidateDirs()) {
    const file = Bun.file(join(dir, ".env"));
    if (!(await file.exists())) continue;
    const vars = parseEnv(await file.text());
    for (const [key, value] of Object.entries(vars)) {
      if (process.env[key] === undefined) {
        process.env[key] = value;
      }
    }
    return; // first .env wins
  }
}
