/**
 * Minimal progress bar renderer using \r overwrites on stderr.
 */

const BAR_WIDTH = 20;
const FILLED = "\u2588"; // █
const EMPTY = "\u2591";  // ░

export class ProgressBar {
  private label: string;
  private total: number;

  constructor(opts: { label: string; total?: number }) {
    this.label = opts.label;
    this.total = opts.total ?? 100;
  }

  /** Update bar from a count (e.g. 3 of 10). */
  update(current: number, total?: number, suffix?: string): void {
    if (total !== undefined) this.total = total;
    const percent = Math.min(100, Math.round((current / this.total) * 100));
    this.render(percent, suffix);
  }

  /** Update bar directly from a 0–100 percentage. */
  updatePercent(percent: number, suffix?: string): void {
    this.render(Math.min(100, Math.round(percent)), suffix);
  }

  /** Finish the bar — writes a newline so the line is preserved. */
  done(message?: string): void {
    if (message) {
      process.stderr.write(`\r\x1b[K${this.label}... ${message}\n`);
    } else {
      this.render(100);
      process.stderr.write("\n");
    }
  }

  private render(percent: number, suffix?: string): void {
    const filled = Math.round((percent / 100) * BAR_WIDTH);
    const empty = BAR_WIDTH - filled;
    const bar = FILLED.repeat(filled) + EMPTY.repeat(empty);
    const extra = suffix ? `  ${suffix}` : "";
    process.stderr.write(`\r\x1b[K${this.label}  [${bar}]  ${percent}%${extra}`);
  }
}
