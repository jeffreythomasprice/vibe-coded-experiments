// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

import Ajv from "ajv";
import type { MountInfo } from "../types/mount-info.js";

// eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-explicit-any
const _validate = new Ajv().compile({
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "MountInfo",
  "title": "MountInfo",
  "description": "Provider mount registration",
  "type": "object",
  "properties": {
    "mountId": {
      "type": "string"
    },
    "scheme": {
      "type": "string",
      "enum": [
        "local",
        "s3",
        "gcs",
        "sftp",
        "smb"
      ]
    },
    "config": {
      "type": "object",
      "additionalProperties": {
        "type": "string"
      }
    }
  },
  "required": [
    "mountId",
    "scheme",
    "config"
  ],
  "additionalProperties": false
} as any);

export function isMountInfo(data: unknown): data is MountInfo {
    return _validate(data) as boolean;
}

export function assertMountInfo(data: unknown): asserts data is MountInfo {
    if (!isMountInfo(data)) {
        throw new Error(`Validation failed: ${JSON.stringify(_validate.errors)}`);
    }
}
