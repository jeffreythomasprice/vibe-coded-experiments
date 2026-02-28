// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

export const localProviderConfigSchema = {
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
} as const;
