// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

import Ajv from "ajv";
import type { MoveRequest } from "../types/move-request.js";

// eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-explicit-any
const _validate = new Ajv().compile({
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "MoveRequest",
  "title": "MoveRequest",
  "description": "File move/rename operation",
  "type": "object",
  "properties": {
    "src": {
      "type": "string",
      "description": "Source URI: <scheme>://<mountId>/<path>"
    },
    "dest": {
      "type": "string",
      "description": "Destination URI"
    }
  },
  "required": [
    "src",
    "dest"
  ],
  "additionalProperties": false
} as any);

export function isMoveRequest(data: unknown): data is MoveRequest {
    return _validate(data) as boolean;
}

export function assertMoveRequest(data: unknown): asserts data is MoveRequest {
    if (!isMoveRequest(data)) {
        throw new Error(`Validation failed: ${JSON.stringify(_validate.errors)}`);
    }
}
