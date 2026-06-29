import { z } from "zod";

export const FEEDBACK_MAX_MESSAGE_LEN = 5_000;
export const FEEDBACK_MAX_NAME_LEN = 200;
export const FEEDBACK_MAX_CONTACT_LEN = 200;

export const feedbackCategorySchema = z.enum(["bug", "feature", "other"]);
export type FeedbackCategory = z.infer<typeof feedbackCategorySchema>;

export const FEEDBACK_CATEGORIES: ReadonlyArray<{
  id: FeedbackCategory;
  label: string;
  description: string;
}> = [
  { id: "bug", label: "Bug", description: "Something is broken" },
  { id: "feature", label: "Feature", description: "An idea or request" },
  { id: "other", label: "Other", description: "Anything else" },
];

export const feedbackMetaSchema = z.object({
  url: z.string().max(512),
  ua: z.string().max(512),
  lang: z.string().max(64),
  build: z.string().max(128),
});

/**
 * Payload sent to `POST /v1/feedback`. The `website` field is a honeypot —
 * it must be empty for legitimate submissions. Bots that auto-fill form
 * fields will populate it and the backend will silently drop the request.
 */
export const feedbackPayloadSchema = z.object({
  category: feedbackCategorySchema,
  message: z.string().min(1).max(FEEDBACK_MAX_MESSAGE_LEN),
  name: z.string().max(FEEDBACK_MAX_NAME_LEN).optional(),
  contact: z.string().max(FEEDBACK_MAX_CONTACT_LEN).optional(),
  meta: feedbackMetaSchema.optional(),
  website: z.string().max(200).optional(),
});
export type FeedbackPayload = z.infer<typeof feedbackPayloadSchema>;

export const feedbackResponseSchema = z.object({
  ok: z.boolean(),
});
export type FeedbackResponse = z.infer<typeof feedbackResponseSchema>;

export const myFeedbackItemSchema = z.object({
  id: z.number(),
  category: z.string(),
  message: z.string(),
  createdAt: z.string(),
  status: z.enum(["open", "acknowledged", "dismissed", "replied"]),
  reply: z.string().nullable(),
});
export type MyFeedbackItem = z.infer<typeof myFeedbackItemSchema>;

export const myFeedbackResponseSchema = z.object({
  items: z.array(myFeedbackItemSchema),
});
