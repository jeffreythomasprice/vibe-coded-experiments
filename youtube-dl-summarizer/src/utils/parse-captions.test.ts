import { test, expect, describe } from "bun:test";
import { parseCaptions } from "./parse-captions.ts";

describe("parseCaptions", () => {
  test("parses VTT format", () => {
    const vtt = `WEBVTT
Kind: captions
Language: en

00:00:00.000 --> 00:00:02.000
Hello world

00:00:02.000 --> 00:00:04.000
This is a test

00:00:04.000 --> 00:00:06.000
Hello world`;

    const result = parseCaptions(vtt);
    // "Hello world" appears twice but not consecutively, so both should remain
    expect(result).toBe("Hello world\nThis is a test\nHello world");
  });

  test("deduplicates consecutive identical lines", () => {
    const vtt = `WEBVTT

00:00:00.000 --> 00:00:01.000
Hello world

00:00:01.000 --> 00:00:02.000
Hello world

00:00:02.000 --> 00:00:03.000
Something new`;

    const result = parseCaptions(vtt);
    expect(result).toBe("Hello world\nSomething new");
  });

  test("strips HTML tags", () => {
    const vtt = `WEBVTT

00:00:00.000 --> 00:00:02.000
<c>Hello</c> <00:00:01.000>world</c>`;

    const result = parseCaptions(vtt);
    expect(result).toBe("Hello world");
  });

  test("parses SRT format", () => {
    const srt = `1
00:00:00,000 --> 00:00:02,000
Hello world

2
00:00:02,000 --> 00:00:04,000
This is a test`;

    const result = parseCaptions(srt);
    expect(result).toBe("Hello world\nThis is a test");
  });

  test("handles empty input", () => {
    expect(parseCaptions("")).toBe("");
    expect(parseCaptions("WEBVTT\n\n")).toBe("");
  });
});
