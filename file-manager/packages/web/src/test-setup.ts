import { mock } from "bun:test";
import { GlobalWindow } from "happy-dom";

// Set up a full DOM environment via happy-dom GlobalWindow.
// Bun 1.3.0's [test] environment bunfig.toml config is not picked up in
// workspace packages, so we bootstrap manually in the preload instead.
const happyWindow = new GlobalWindow({ url: "http://localhost/" });

// Set specific globals needed by @testing-library/react and happy-dom internals.
// We must set these BEFORE any test module imports so happy-dom's `this.window`
// references (e.g. in SelectorParser) resolve to the same object as globalThis.window.
const domGlobals = [
    "window", "document", "navigator", "location", "history",
    "HTMLElement", "Element", "Node", "NodeList", "Text", "Comment",
    "DocumentFragment", "Event", "MouseEvent", "KeyboardEvent",
    "CustomEvent", "MutationObserver", "IntersectionObserver",
    "ResizeObserver", "ShadowRoot", "Attr", "NamedNodeMap",
    "HTMLAnchorElement", "HTMLButtonElement", "HTMLDivElement",
    "HTMLInputElement", "HTMLFormElement", "HTMLLabelElement",
    "HTMLSelectElement", "HTMLTextAreaElement", "HTMLTableElement",
    "HTMLCollection", "Range", "TreeWalker", "XMLSerializer",
    "DOMParser", "URL", "URLSearchParams",
    "SyntaxError", "TypeError", "RangeError", "Error",
] as const;

for (const key of domGlobals) {
    // @ts-ignore
    if (key in happyWindow && happyWindow[key] !== undefined) {
        try {
            // @ts-ignore
            globalThis[key] = happyWindow[key];
        } catch {
            // read-only — skip
        }
    }
}

// Stub CSS module imports so component tests don't fail on .css files.
mock.module("../components/Modal.module.css", () => ({ default: new Proxy({}, { get: () => "" }) }));
