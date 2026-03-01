import { describe, test, expect } from "bun:test";
import { parseUri } from "./client.js";

describe("parseUri", () => {
    test("valid URI with path", () => {
        const result = parseUri("docs://reports/q1.pdf");
        expect(result).toEqual({ mountId: "docs", path: "/reports/q1.pdf" });
    });

    test("valid URI with trailing slash (root path)", () => {
        const result = parseUri("docs://");
        expect(result).toEqual({ mountId: "docs", path: "/" });
    });

    test("URI with no trailing slash defaults to root", () => {
        const result = parseUri("docs://");
        expect(result).toEqual({ mountId: "docs", path: "/" });
    });

    test("throws on invalid format (no scheme separator)", () => {
        expect(() => parseUri("not-a-uri")).toThrow("Invalid URI");
    });

    test("preserves nested path segments", () => {
        const result = parseUri("myserver://folder/sub/file.txt");
        expect(result).toEqual({ mountId: "myserver", path: "/folder/sub/file.txt" });
    });

    test("mountId with hyphens, underscores, and digits", () => {
        const result = parseUri("my-mount_2://file.txt");
        expect(result).toEqual({ mountId: "my-mount_2", path: "/file.txt" });
    });
});
