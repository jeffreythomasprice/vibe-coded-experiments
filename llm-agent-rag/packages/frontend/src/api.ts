import { RagClient } from "@rag/shared";

export const api = new RagClient(import.meta.env.VITE_API_URL ?? "http://127.0.0.1:8001");
