import { z } from "zod/v4";

export const messageRequestSchema = z.object({
  message: z.string().min(1),
});

export type MessageRequest = z.infer<typeof messageRequestSchema>;
