import { test, expect, describe } from "bun:test";
import { parseTimestamps } from "./extract-frames.ts";

describe("parseTimestamps", () => {
  test("extracts timestamps from summary text", () => {
    const summary = `## Key Points
- [00:05:23] Introduction to the topic
- [00:12:45] Main argument presented
- [00:30:00] Conclusion`;

    expect(parseTimestamps(summary)).toEqual(["00:05:23", "00:12:45", "00:30:00"]);
  });

  test("deduplicates timestamps", () => {
    const summary = `[00:05:23] mentioned here and [00:05:23] mentioned again`;
    expect(parseTimestamps(summary)).toEqual(["00:05:23"]);
  });

  test("caps at 15 timestamps", () => {
    const lines = Array.from({ length: 20 }, (_, i) => {
      const m = String(i).padStart(2, "0");
      return `- [00:${m}:00] Point ${i}`;
    });
    const summary = lines.join("\n");
    expect(parseTimestamps(summary)).toHaveLength(15);
  });

  test("returns empty array for no timestamps", () => {
    expect(parseTimestamps("No timestamps here")).toEqual([]);
  });

  test("ignores malformed timestamps", () => {
    const summary = `[00:05] too short [0:5:23] wrong format [00:05:23] valid`;
    expect(parseTimestamps(summary)).toEqual(["00:05:23"]);
  });
});
