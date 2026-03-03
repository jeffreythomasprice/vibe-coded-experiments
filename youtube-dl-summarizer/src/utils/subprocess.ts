/**
 * Subprocess utilities wrapping Bun.spawn with stdout/stderr capture.
 */

export function ensureBinary(name: string, installHint?: string): string {
  const path = Bun.which(name);
  if (!path) {
    const hint = installHint ? `\nInstall: ${installHint}` : "";
    throw new Error(`"${name}" is not installed or not on your PATH.${hint}`);
  }
  return path;
}

export interface RunResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

export async function run(
  command: string[],
  opts?: { cwd?: string; stdin?: string },
): Promise<RunResult> {
  const proc = Bun.spawn(command, {
    cwd: opts?.cwd,
    stdin: opts?.stdin ? new TextEncoder().encode(opts.stdin) : "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });

  const [stdout, stderr] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);

  const exitCode = await proc.exited;

  if (exitCode !== 0) {
    const cmd = command.join(" ");
    throw new Error(
      `Command failed (exit ${exitCode}): ${cmd}\n${stderr.trim()}`,
    );
  }

  return { stdout, stderr, exitCode };
}
