// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

import Ajv from "ajv";
import type { CliConfig } from "../types/cli-config.js";

// eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-explicit-any
const _validate = new Ajv().compile({
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "CliConfig",
  "title": "CliConfig",
  "description": "Persistent configuration for the file-manager CLI",
  "type": "object",
  "properties": {
    "apiUrl": {
      "type": "string",
      "description": "Base URL of the file-manager API server"
    }
  },
  "additionalProperties": false
} as any);

export function isCliConfig(data: unknown): data is CliConfig {
    return _validate(data) as boolean;
}

export function assertCliConfig(data: unknown): asserts data is CliConfig {
    if (!isCliConfig(data)) {
        throw new Error(`Validation failed: ${JSON.stringify(_validate.errors)}`);
    }
}
