// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

export const cliConfigSchema = {
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
} as const;
