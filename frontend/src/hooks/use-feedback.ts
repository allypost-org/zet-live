import { API_URL, FRONTEND_VERSION_ID } from "@/app/consts";
import { apiFetch } from "@/app/entity/v1/api";
import { feedbackResponseSchema, type FeedbackPayload } from "@/app/entity/v1/feedback";
import { toast } from "sonner";
import { closeFeedback } from "@/feedback-store";

export type SubmitFeedbackResult = {
  ok: boolean;
  errorMessage: string | null;
};

function buildMetadata(): FeedbackPayload["meta"] {
  if (typeof window === "undefined") return undefined;
  const navigator = window.navigator;
  return {
    url: window.location.href.slice(0, 512),
    ua: navigator.userAgent.slice(0, 512),
    lang: navigator.language.slice(0, 64),
    build: FRONTEND_VERSION_ID ?? "development",
  };
}

export async function submitFeedback(
  payload: Omit<FeedbackPayload, "meta">,
  signal?: AbortSignal,
): Promise<SubmitFeedbackResult> {
  const body: FeedbackPayload = {
    ...payload,
    meta: buildMetadata(),
  };

  const result = await apiFetch(`${API_URL}/v1/feedback`, feedbackResponseSchema, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
    signal,
  });

  if (result.error) {
    return { ok: false, errorMessage: result.error.error };
  }

  return { ok: true, errorMessage: null };
}

export function useFeedbackSubmit() {
  return async (payload: Omit<FeedbackPayload, "meta">): Promise<boolean> => {
    const result = await submitFeedback(payload);
    if (result.ok) {
      toast.success("Thanks for your feedback!");
      closeFeedback();
      return true;
    }
    toast.error("Failed to submit feedback", {
      description: result.errorMessage ?? undefined,
    });
    return false;
  };
}
