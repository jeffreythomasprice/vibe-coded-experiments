import type { Context, Next } from "koa";
import { firstError } from "@rag/shared";

interface ValidateFn {
  (data: unknown): boolean;
  errors?: Array<{ instancePath?: string; message?: string }> | null;
}

export function validateBody(validate: ValidateFn) {
  return async (ctx: Context, next: Next) => {
    if (validate(ctx.request.body)) {
      await next();
    } else {
      ctx.status = 400;
      ctx.body = { error: firstError(validate.errors) };
    }
  };
}
