// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

export const moveRequestSchema = {
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
} as const;
