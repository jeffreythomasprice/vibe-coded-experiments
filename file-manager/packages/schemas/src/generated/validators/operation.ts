// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

import Ajv from "ajv";
import type { Operation } from "../types/operation.js";

// eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-explicit-any
const _validate = new Ajv().compile({
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "Operation",
  "title": "Operation",
  "description": "Tracks progress of a file operation (copy, move, upload, or delete)",
  "type": "object",
  "properties": {
    "id": {
      "type": "string"
    },
    "type": {
      "type": "string",
      "enum": [
        "copy",
        "move",
        "upload",
        "delete"
      ]
    },
    "status": {
      "type": "string",
      "enum": [
        "pending",
        "in_progress",
        "completed",
        "failed"
      ]
    },
    "src": {
      "type": "string"
    },
    "dest": {
      "type": "string"
    },
    "totalBytes": {
      "type": "number"
    },
    "processedBytes": {
      "type": "number"
    },
    "totalFiles": {
      "type": "number"
    },
    "processedFiles": {
      "type": "number"
    },
    "currentFile": {
      "type": "string"
    },
    "error": {
      "type": "string"
    },
    "createdAt": {
      "type": "string",
      "description": "ISO 8601"
    },
    "updatedAt": {
      "type": "string",
      "description": "ISO 8601"
    }
  },
  "required": [
    "id",
    "type",
    "status",
    "src",
    "totalBytes",
    "processedBytes",
    "totalFiles",
    "processedFiles",
    "createdAt",
    "updatedAt"
  ],
  "additionalProperties": false
} as any);

export function isOperation(data: unknown): data is Operation {
    return _validate(data) as boolean;
}

export function assertOperation(data: unknown): asserts data is Operation {
    if (!isOperation(data)) {
        throw new Error(`Validation failed: ${JSON.stringify(_validate.errors)}`);
    }
}
