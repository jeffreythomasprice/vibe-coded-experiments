let silent = false;

export function setSilent(v: boolean) {
  silent = v;
}

export function print(...args: unknown[]) {
  if (!silent) console.log(...args);
}

export function printError(...args: unknown[]) {
  if (!silent) console.error(...args);
}

export function printProgress(msg: string) {
  if (!silent) process.stdout.write(msg);
}
