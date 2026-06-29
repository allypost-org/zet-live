import { z } from "zod";

export const providerPublicSchema = z.object({
  id: z.string(),
  name: z.string(),
});

export const capabilitiesSchema = z.object({
  appUrl: z.string().nullable(),
  auth: z.object({
    providers: z.array(providerPublicSchema),
  }),
});

export type ProviderPublic = z.infer<typeof providerPublicSchema>;
export type Capabilities = z.infer<typeof capabilitiesSchema>;

export const identitySchema = z.object({
  provider: z.string(),
  email: z.string().nullable(),
  displayName: z.string().nullable(),
  avatarUrl: z.string().nullable(),
});

export const userSchema = z.object({
  id: z.string(),
  displayName: z.string().nullable(),
  email: z.string().nullable(),
  avatarUrl: z.string().nullable(),
});

export const meResponseSchema = z.object({
  user: userSchema,
  identities: z.array(identitySchema),
  sessionId: z.string(),
});

export const sessionInfoSchema = z.object({
  id: z.string(),
  userId: z.string(),
  createdAt: z.string(),
  expiresAt: z.string(),
  ip: z.string().nullable(),
  userAgent: z.string().nullable(),
});

export const sessionListResponseSchema = z.object({
  sessions: z.array(sessionInfoSchema),
});

export const okResponseSchema = z.object({
  ok: z.boolean(),
});

export const revokeAllResponseSchema = z.object({
  revoked: z.number(),
});

export const linkTicketResponseSchema = z.object({
  ticket: z.string(),
});

export type User = z.infer<typeof userSchema>;
export type Identity = z.infer<typeof identitySchema>;
export type MeResponse = z.infer<typeof meResponseSchema>;
export type SessionInfo = z.infer<typeof sessionInfoSchema>;
