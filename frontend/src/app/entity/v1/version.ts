import { z } from "zod";

export const frontendVersionIdSchema = z.string().regex(/^[a-f0-9]{64}$/);

export const versionResponseSchema = z.object({
  frontendId: frontendVersionIdSchema.nullable(),
});
