import { describe, test, expect } from "bun:test";
import { parseUri } from "./client.js";

describe("parseUri", () => {
    test("valid URI with path", () => {
        const result = parseUri("local://docs/reports/q1.pdf");
        expect(result).toEqual({ scheme: "local", mountId: "docs", path: "/reports/q1.pdf" });
    });

    test("valid URI with trailing slash (root path)", () => {
        const result = parseUri("local://docs/");
        expect(result).toEqual({ scheme: "local", mountId: "docs", path: "/" });
    });

    test("URI with no path defaults to root", () => {
        const result = parseUri("local://docs");
        expect(result).toEqual({ scheme: "local", mountId: "docs", path: "/" });
    });

    test("throws on invalid format (no scheme)", () => {
        expect(() => parseUri("not-a-uri")).toThrow("Invalid URI");
    });

    test("throws on missing mountId", () => {
        // local:///path has an empty string for mountId after the double-slash
        // The regex requires at least one char in ([^/]+) so this should fail to match
        expect(() => parseUri("local:///path")).toThrow("Invalid URI");
    });

    test("preserves nested path segments", () => {
        const result = parseUri("sftp://myserver/folder/sub/file.txt");
        expect(result).toEqual({ scheme: "sftp", mountId: "myserver", path: "/folder/sub/file.txt" });
    });
});
