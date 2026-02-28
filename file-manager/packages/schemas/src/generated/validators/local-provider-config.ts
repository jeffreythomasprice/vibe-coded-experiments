// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

import Ajv from "ajv";
import type { LocalProviderConfig } from "../types/local-provider-config.js";

// eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-explicit-any
const _validate = new Ajv().compile({
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "LocalProviderConfig",
  "title": "LocalProviderConfig",
  "description": "Configuration for the local filesystem provider",
  "type": "object",
  "properties": {
    "rootDir": {
      "type": "string",
      "description": "Absolute path to the root directory"
    }
  },
  "required": [
    "rootDir"
  ],
  "additionalProperties": false
} as any);

export function isLocalProviderConfig(data: unknown): data is LocalProviderConfig {
    return _validate(data) as boolean;
}

export function assertLocalProviderConfig(data: unknown): asserts data is LocalProviderConfig {
    if (!isLocalProviderConfig(data)) {
        throw new Error(`Validation failed: ${JSON.stringify(_validate.errors)}`);
    }
}
