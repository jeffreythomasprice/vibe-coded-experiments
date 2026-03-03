import { test, expect, describe } from "bun:test";
import { extractVideoId } from "./cache.ts";

describe("extractVideoId", () => {
  test("standard watch URL", () => {
    expect(extractVideoId("https://www.youtube.com/watch?v=dQw4w9WgXcQ")).toBe(
      "dQw4w9WgXcQ",
    );
  });

  test("watch URL without www", () => {
    expect(extractVideoId("https://youtube.com/watch?v=dQw4w9WgXcQ")).toBe(
      "dQw4w9WgXcQ",
    );
  });

  test("watch URL with extra params", () => {
    expect(
      extractVideoId("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=120"),
    ).toBe("dQw4w9WgXcQ");
  });

  test("short URL (youtu.be)", () => {
    expect(extractVideoId("https://youtu.be/dQw4w9WgXcQ")).toBe(
      "dQw4w9WgXcQ",
    );
  });

  test("short URL with params", () => {
    expect(extractVideoId("https://youtu.be/dQw4w9WgXcQ?t=30")).toBe(
      "dQw4w9WgXcQ",
    );
  });

  test("embed URL", () => {
    expect(
      extractVideoId("https://www.youtube.com/embed/dQw4w9WgXcQ"),
    ).toBe("dQw4w9WgXcQ");
  });

  test("shorts URL", () => {
    expect(
      extractVideoId("https://www.youtube.com/shorts/dQw4w9WgXcQ"),
    ).toBe("dQw4w9WgXcQ");
  });

  test("mobile URL", () => {
    expect(
      extractVideoId("https://m.youtube.com/watch?v=dQw4w9WgXcQ"),
    ).toBe("dQw4w9WgXcQ");
  });

  test("throws on invalid URL", () => {
    expect(() => extractVideoId("not-a-url")).toThrow("Invalid URL");
  });

  test("throws on non-YouTube URL", () => {
    expect(() => extractVideoId("https://example.com/watch?v=abc")).toThrow(
      "Could not extract video ID",
    );
  });
});
