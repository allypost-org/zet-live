import { z } from "zod";

export const noticeSeveritySchema = z.enum(["info", "warning", "error"]);
export type NoticeSeverity = z.infer<typeof noticeSeveritySchema>;

export const globalNoticeSchema = z.object({
  id: z.string(),
  text: z.string(),
  severity: noticeSeveritySchema,
});
export type GlobalNotice = z.infer<typeof globalNoticeSchema>;

export const adminSettingsSchema = z.object({
  realtimeUrl: z.string().nullable().optional(),
  staticUrl: z.string().nullable().optional(),
  gbfsUrl: z.string().nullable().optional(),
  realtimePaused: z.boolean().nullable().optional(),
  staticPaused: z.boolean().nullable().optional(),
  gbfsPaused: z.boolean().nullable().optional(),
  globalNotices: z.array(globalNoticeSchema).default([]),
});
export type AdminSettings = z.infer<typeof adminSettingsSchema>;

export const metadataStatusSchema = z.enum([
  "in-progress",
  "success",
  "error",
  "skipped",
  "paused",
]);
export type MetadataStatus = z.infer<typeof metadataStatusSchema>;

export const metadataEntrySchema = z.object({
  status: metadataStatusSchema,
  lastSyncAt: z.string(),
  errorMessage: z.string().nullable().optional(),
  recordsProcessed: z.number().nullable().optional(),
  durationMs: z.number().nullable().optional(),
});
export type MetadataEntry = z.infer<typeof metadataEntrySchema>;

export const metadataMapSchema = z.record(z.string(), metadataEntrySchema);
export type MetadataMap = z.infer<typeof metadataMapSchema>;

export const feedbackRowSchema = z.object({
  id: z.number(),
  category: z.string(),
  message: z.string(),
  name: z.string().nullable().optional(),
  contact: z.string().nullable().optional(),
  metaUrl: z.string().nullable().optional(),
  metaUa: z.string().nullable().optional(),
  metaLang: z.string().nullable().optional(),
  metaBuild: z.string().nullable().optional(),
  ip: z.string(),
  createdAt: z.string(),
  handled: z.boolean(),
  dismissed: z.boolean(),
  status: z.string(),
  reply: z.string().nullable().optional(),
  repliedAt: z.string().nullable().optional(),
  userId: z.string().nullable().optional(),
  userEmail: z.string().nullable().optional(),
  userDisplayName: z.string().nullable().optional(),
});
export type FeedbackRow = z.infer<typeof feedbackRowSchema>;

export const authProviderSchema = z.object({
  id: z.string(),
  name: z.string(),
  clientId: z.string(),
  enabled: z.boolean(),
});

export const authPresetSchema = z.object({
  id: z.string(),
  name: z.string(),
});

export const authProvidersResponseSchema = z.object({
  providers: z.array(authProviderSchema),
  presets: z.array(authPresetSchema),
});
export type AuthProvider = z.infer<typeof authProviderSchema>;
export type AuthPreset = z.infer<typeof authPresetSchema>;

export const userSummarySchema = z.object({
  id: z.string(),
  displayName: z.string().nullable().optional(),
  email: z.string().nullable().optional(),
  providers: z.array(z.string()),
  createdAt: z.string(),
  noticeCount: z.number(),
});
export type UserSummary = z.infer<typeof userSummarySchema>;

export function pageSchema<T extends z.ZodTypeAny>(itemSchema: T) {
  return z.object({ items: itemSchema.array(), nextCursor: z.string().nullable() });
}

export const sessionInfoSchema = z.object({
  id: z.string(),
  userId: z.string(),
  createdAt: z.string(),
  expiresAt: z.string(),
  ip: z.string().nullable().optional(),
  userAgent: z.string().nullable().optional(),
});
export type SessionInfo = z.infer<typeof sessionInfoSchema>;
export const adminSessionInfoSchema = sessionInfoSchema.extend({
  userDisplayName: z.string().nullable(),
  userEmail: z.string().nullable(),
});
export type AdminSessionInfo = z.infer<typeof adminSessionInfoSchema>;

export const globalNoticeForUserSchema = z.object({
  id: z.string(),
  text: z.string(),
  severity: noticeSeveritySchema,
});

export const userDetailSchema = z.object({
  id: z.string(),
  displayName: z.string().nullable().optional(),
  email: z.string().nullable().optional(),
  providers: z.array(z.string()),
  createdAt: z.string(),
  noticeCount: z.number(),
  sessions: z.array(sessionInfoSchema),
  notices: z.array(globalNoticeForUserSchema),
  feedback: z.array(feedbackRowSchema),
});
export type UserDetail = z.infer<typeof userDetailSchema>;

export const userEditSchema = z.object({
  displayName: z.string(),
  email: z.string(),
});
export type UserEdit = z.infer<typeof userEditSchema>;

export const providerFormSchema = z.object({
  id: z.string().min(1),
  clientId: z.string().min(1),
  clientSecret: z.string().min(1),
});
export type ProviderFormValues = z.infer<typeof providerFormSchema>;

export const noticeCreateSchema = z.object({
  text: z.string().min(1),
  severity: noticeSeveritySchema,
});
export type NoticeCreateValues = z.infer<typeof noticeCreateSchema>;

export const userNoticeRowSchema = z.object({
  id: z.string(),
  userId: z.string(),
  userEmail: z.string().nullable().optional(),
  userDisplayName: z.string().nullable().optional(),
  text: z.string(),
  severity: noticeSeveritySchema,
  createdAt: z.string(),
});
export type UserNoticeRow = z.infer<typeof userNoticeRowSchema>;

export const connectionsSchema = z.record(z.string(), z.number());
export type Connections = z.infer<typeof connectionsSchema>;

export const toastTypeSchema = z.enum(["info", "success", "warning", "error"]);
export type ToastType = z.infer<typeof toastTypeSchema>;

export const notificationTargetSchema = z.enum(["all", "ips", "account"]);
export type NotificationTarget = z.infer<typeof notificationTargetSchema>;

export const toastPayloadSchema = z.object({
  message: z.string(),
  type: toastTypeSchema.default("info"),
  duration: z.number().nullable().optional(),
  target: notificationTargetSchema.default("all"),
  ips: z.array(z.string()).optional(),
  account: z.string().nullable().optional(),
});
export type ToastPayload = z.infer<typeof toastPayloadSchema>;

export const feedbackFilterSchema = z.enum(["all", "new", "archived"]);
export type FeedbackFilter = z.infer<typeof feedbackFilterSchema>;

export const flagStateSchema = z.enum(["disabled", "enabled", "loggedIn", "scoped"]);
export const flagStates = flagStateSchema.options;
export type FlagState = z.infer<typeof flagStateSchema>;

export const scopedUserSchema = z.object({
  id: z.string(),
  displayName: z.string().nullable().optional(),
  email: z.string().nullable().optional(),
  avatarUrl: z.string().nullable().optional(),
});
export type ScopedUser = z.infer<typeof scopedUserSchema>;

export const featureFlagRowSchema = z.object({
  id: z.number(),
  key: z.string(),
  description: z.string(),
  state: flagStateSchema,
  updatedAt: z.string(),
  orphaned: z.boolean(),
  scopedUsers: z.array(scopedUserSchema),
});
export type FeatureFlagRow = z.infer<typeof featureFlagRowSchema>;

export const featureFlagUpdateSchema = z.object({
  state: flagStateSchema,
  description: z.string(),
  userIds: z.array(z.string()),
});
export type FeatureFlagUpdate = z.infer<typeof featureFlagUpdateSchema>;
