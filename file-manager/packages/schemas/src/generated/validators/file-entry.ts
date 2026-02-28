// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

import Ajv from "ajv";
import type { FileEntry } from "../types/file-entry.js";

// eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-explicit-any
const _validate = new Ajv().compile({
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "FileEntry",
  "title": "FileEntry",
  "description": "A single entry in a file listing (JSON-serializable form)",
  "type": "object",
  "properties": {
    "name": {
      "type": "string",
      "description": "Filename or directory name (basename)"
    },
    "path": {
      "type": "string",
      "description": "Full path within the mount"
    },
    "type": {
      "type": "string",
      "enum": [
        "file",
        "directory",
        "symlink"
      ],
      "description": "Entry type"
    },
    "size": {
      "type": "number",
      "description": "Size in bytes"
    },
    "modifiedAt": {
      "type": "string",
      "description": "Last-modified timestamp (ISO 8601)"
    }
  },
  "required": [
    "name",
    "path",
    "type",
    "size",
    "modifiedAt"
  ],
  "additionalProperties": false
} as any);

export function isFileEntry(data: unknown): data is FileEntry {
    return _validate(data) as boolean;
}

export function assertFileEntry(data: unknown): asserts data is FileEntry {
    if (!isFileEntry(data)) {
        throw new Error(`Validation failed: ${JSON.stringify(_validate.errors)}`);
    }
}
