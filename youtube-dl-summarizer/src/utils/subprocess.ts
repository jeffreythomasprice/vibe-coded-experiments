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
  opts?: { cwd?: string; stdin?: string; streamStderr?: boolean },
): Promise<RunResult> {
  const proc = Bun.spawn(command, {
    cwd: opts?.cwd,
    stdin: opts?.stdin ? new TextEncoder().encode(opts.stdin) : "ignore",
    stdout: "pipe",
    stderr: opts?.streamStderr ? "pipe" : "pipe",
  });

  let stderrText: string;
  if (opts?.streamStderr && proc.stderr) {
    // Tee stderr to process.stderr while capturing it
    const chunks: Uint8Array[] = [];
    const reader = proc.stderr.getReader();
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      process.stderr.write(value);
    }
    stderrText = new TextDecoder().decode(Buffer.concat(chunks));
  } else {
    stderrText = await new Response(proc.stderr).text();
  }

  const stdout = await new Response(proc.stdout).text();
  const exitCode = await proc.exited;

  if (exitCode !== 0) {
    const cmd = command.join(" ");
    throw new Error(
      `Command failed (exit ${exitCode}): ${cmd}\n${stderrText.trim()}`,
    );
  }

  return { stdout, stderr: stderrText, exitCode };
}
