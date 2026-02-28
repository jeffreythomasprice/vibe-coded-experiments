// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

import Ajv from "ajv";
import type { FileStat } from "../types/file-stat.js";

// eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-explicit-any
const _validate = new Ajv().compile({
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "FileStat",
  "title": "FileStat",
  "description": "Metadata for a single file or directory (timestamps as ISO 8601 strings)",
  "type": "object",
  "properties": {
    "name": {
      "type": "string"
    },
    "path": {
      "type": "string"
    },
    "type": {
      "type": "string",
      "enum": [
        "file",
        "directory",
        "symlink"
      ]
    },
    "size": {
      "type": "number",
      "description": "Size in bytes"
    },
    "createdAt": {
      "type": "string",
      "description": "ISO 8601"
    },
    "modifiedAt": {
      "type": "string",
      "description": "ISO 8601"
    },
    "mimeType": {
      "type": "string"
    }
  },
  "required": [
    "name",
    "path",
    "type",
    "size",
    "createdAt",
    "modifiedAt"
  ],
  "additionalProperties": false
} as any);

export function isFileStat(data: unknown): data is FileStat {
    return _validate(data) as boolean;
}

export function assertFileStat(data: unknown): asserts data is FileStat {
    if (!isFileStat(data)) {
        throw new Error(`Validation failed: ${JSON.stringify(_validate.errors)}`);
    }
}
