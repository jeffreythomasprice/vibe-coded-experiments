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
  opts?: {
    cwd?: string;
    stdin?: string;
    streamStderr?: boolean;
    onStderr?: (chunk: string) => void;
    onStdout?: (chunk: string) => void;
  },
): Promise<RunResult> {
  const streamingStderr = opts?.streamStderr || opts?.onStderr;
  const streamingStdout = opts?.onStdout;

  const proc = Bun.spawn(command, {
    cwd: opts?.cwd,
    stdin: opts?.stdin ? new TextEncoder().encode(opts.stdin) : "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });

  // Read stdout and stderr concurrently to avoid pipe deadlocks
  async function readStream(
    stream: ReadableStream<Uint8Array> | null,
    callback?: (chunk: string) => void,
    teeToStderr?: boolean,
  ): Promise<string> {
    if (!stream) return "";
    if (!callback && !teeToStderr) {
      return new Response(stream).text();
    }
    const chunks: Uint8Array[] = [];
    const reader = stream.getReader();
    const decoder = new TextDecoder();
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      if (callback) {
        callback(decoder.decode(value, { stream: true }));
      } else if (teeToStderr) {
        process.stderr.write(value);
      }
    }
    return new TextDecoder().decode(Buffer.concat(chunks));
  }

  const [stdout, stderrText] = await Promise.all([
    readStream(proc.stdout, streamingStdout ? opts!.onStdout : undefined),
    readStream(
      proc.stderr,
      streamingStderr ? opts?.onStderr : undefined,
      streamingStderr && !opts?.onStderr,
    ),
  ]);

  const exitCode = await proc.exited;

  if (exitCode !== 0) {
    const cmd = command.join(" ");
    throw new Error(
      `Command failed (exit ${exitCode}): ${cmd}\n${stderrText.trim()}`,
    );
  }

  return { stdout, stderr: stderrText, exitCode };
}
