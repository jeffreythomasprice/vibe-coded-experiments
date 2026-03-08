import Ajv from "ajv";
import {
  IngestRequestSchema,
  QueryRequestSchema,
  AgentRequestSchema,
  FindDocumentsRequestSchema,
  AddTagRequestSchema,
} from "./generated/schemas.js";

const ajv = new Ajv();

export const validateIngestRequest = ajv.compile(IngestRequestSchema);
export const validateQueryRequest = ajv.compile(QueryRequestSchema);
export const validateAgentRequest = ajv.compile(AgentRequestSchema);
export const validateFindDocumentsRequest = ajv.compile(FindDocumentsRequestSchema);
export const validateAddTagRequest = ajv.compile(AddTagRequestSchema);

export function firstError(errors: Array<{ instancePath?: string; message?: string }> | null | undefined): string {
  const e = errors?.[0];
  return e ? `${e.instancePath || "/"}: ${e.message}` : "Invalid request body";
}
