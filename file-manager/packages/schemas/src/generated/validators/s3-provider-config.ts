// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate

import Ajv from "ajv";
import type { S3ProviderConfig } from "../types/s3-provider-config.js";

// eslint-disable-next-line @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-explicit-any
const _validate = new Ajv().compile({
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "S3ProviderConfig",
  "title": "S3ProviderConfig",
  "description": "Configuration for the S3 storage provider",
  "type": "object",
  "properties": {
    "bucket": {
      "type": "string",
      "description": "S3 bucket name"
    },
    "prefix": {
      "type": "string",
      "description": "Key prefix (virtual directory) within the bucket"
    },
    "region": {
      "type": "string",
      "description": "AWS region (e.g. us-east-1)"
    },
    "accessKeyId": {
      "type": "string",
      "description": "AWS access key ID (omit to use default credential chain)"
    },
    "secretAccessKey": {
      "type": "string",
      "description": "AWS secret access key (omit to use default credential chain)"
    }
  },
  "required": [
    "bucket"
  ],
  "additionalProperties": false
} as any);

export function isS3ProviderConfig(data: unknown): data is S3ProviderConfig {
    return _validate(data) as boolean;
}

export function assertS3ProviderConfig(data: unknown): asserts data is S3ProviderConfig {
    if (!isS3ProviderConfig(data)) {
        throw new Error(`Validation failed: ${JSON.stringify(_validate.errors)}`);
    }
}
