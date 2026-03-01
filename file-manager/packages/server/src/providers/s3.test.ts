import { describe, expect, it } from "bun:test";
import { resolveS3Key, s3RelativePath } from "./s3.js";

describe("resolveS3Key", () => {
    it("returns path as-is when prefix is empty", () => {
        expect(resolveS3Key("", "foo/bar.txt")).toBe("foo/bar.txt");
    });

    it("strips leading slashes from path", () => {
        expect(resolveS3Key("", "/foo/bar.txt")).toBe("foo/bar.txt");
    });

    it("prepends prefix to path", () => {
        expect(resolveS3Key("data", "foo/bar.txt")).toBe("data/foo/bar.txt");
    });

    it("trims trailing slashes from prefix", () => {
        expect(resolveS3Key("data/", "foo.txt")).toBe("data/foo.txt");
        expect(resolveS3Key("data///", "foo.txt")).toBe("data/foo.txt");
    });

    it("returns prefix/ when path is empty", () => {
        expect(resolveS3Key("data", "")).toBe("data/");
    });

    it("returns empty string when both are empty", () => {
        expect(resolveS3Key("", "")).toBe("");
    });

    it("handles nested prefix", () => {
        expect(resolveS3Key("a/b/c", "d.txt")).toBe("a/b/c/d.txt");
    });
});

describe("s3RelativePath", () => {
    it("returns key as-is when prefix is empty", () => {
        expect(s3RelativePath("", "foo/bar.txt")).toBe("foo/bar.txt");
    });

    it("strips the prefix from a key", () => {
        expect(s3RelativePath("data", "data/foo/bar.txt")).toBe("foo/bar.txt");
    });

    it("strips prefix with trailing slash", () => {
        expect(s3RelativePath("data/", "data/foo.txt")).toBe("foo.txt");
    });

    it("returns key unchanged if prefix does not match", () => {
        expect(s3RelativePath("other", "data/foo.txt")).toBe("data/foo.txt");
    });

    it("handles nested prefix", () => {
        expect(s3RelativePath("a/b/c", "a/b/c/d.txt")).toBe("d.txt");
    });
});
