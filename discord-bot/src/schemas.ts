import { z } from "zod/v4";

export const messageRequestSchema = z.object({
  message: z.string().min(1),
});

export type MessageRequest = z.infer<typeof messageRequestSchema>;

export const notificationRequestSchema = z.object({
  session_id: z.string().optional(),
  transcript_path: z.string().optional(),
  cwd: z.string().optional(),
  hook_event_name: z.string().optional(),
  message: z.string().optional(),
  notification_type: z.string().optional(),
});

export type NotificationRequest = z.infer<typeof notificationRequestSchema>;
