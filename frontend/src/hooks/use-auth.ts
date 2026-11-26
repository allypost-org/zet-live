import { useEffect } from "react";
import { API_URL } from "@/app/consts";
import { apiFetch } from "@/app/entity/v1/api";
import { linkTicketResponseSchema, meResponseSchema, okResponseSchema } from "@/app/entity/v1/auth";
import { clearAuth, setAuth, setSessionToken, sessionToken } from "@/auth-store";
import { toast } from "sonner";
import { getT } from "@/i18n";
import { openOAuthPopup } from "@/utils/oauth-popup";

async function fetchMe(): Promise<void> {
  if (sessionToken() === null) {
    setAuth(null, [], null);
    return;
  }
  const result = await apiFetch(`${API_URL}/v1/auth/me`, meResponseSchema);
  if (result.data) {
    setAuth(result.data.user, result.data.identities, result.data.sessionId);
  } else {
    // Token invalid/expired — drop it.
    clearAuth();
  }
}

export function useAuth() {
  useEffect(() => {
    void fetchMe();
  }, []);
}

export async function loginWith(provider: string): Promise<boolean> {
  try {
    const result = await openOAuthPopup(provider);
    if (!result.ok || !result.token) {
      toast.error(getT()("auth.toast.signInFailed"), { description: result.error ?? undefined });
      return false;
    }
    setSessionToken(result.token);
    await fetchMe();
    return true;
  } catch (e) {
    toast.error(getT()("auth.toast.signInFailed"), {
      description: e instanceof Error ? e.message : undefined,
    });
    return false;
  }
}

export async function linkProvider(
  provider: string,
): Promise<
  { status: "linked" } | { status: "conflict"; transferToken: string } | { status: "error" }
> {
  const ticketRes = await apiFetch(`${API_URL}/v1/auth/link-ticket`, linkTicketResponseSchema, {
    method: "POST",
  });
  if (!ticketRes.data) {
    toast.error(getT()("auth.toast.linkStartFailed"), {
      description: ticketRes.error.error || undefined,
    });
    return { status: "error" };
  }

  try {
    const result = await openOAuthPopup(provider, true, ticketRes.data.ticket);
    if (result.conflict && result.transferToken) {
      return { status: "conflict", transferToken: result.transferToken };
    }
    if (!result.ok) {
      toast.error(getT()("auth.toast.linkFailed"), { description: result.error ?? undefined });
      return { status: "error" };
    }
    await fetchMe();
    return { status: "linked" };
  } catch (e) {
    toast.error(getT()("auth.toast.linkFailed"), {
      description: e instanceof Error ? e.message : undefined,
    });
    return { status: "error" };
  }
}

export async function confirmTransfer(token: string): Promise<boolean> {
  const res = await apiFetch(`${API_URL}/v1/auth/transfer`, okResponseSchema, {
    method: "POST",
    body: JSON.stringify({ token }),
  });
  if (res.data) {
    await fetchMe();
    return true;
  }
  toast.error(getT()("auth.toast.transferFailed"), { description: res.error.error || undefined });
  return false;
}

export async function logout(): Promise<void> {
  await apiFetch(`${API_URL}/v1/auth/logout`, okResponseSchema, { method: "POST" });
  clearAuth();
}

export async function deleteAccount(): Promise<boolean> {
  const res = await apiFetch(`${API_URL}/v1/auth/account`, okResponseSchema, {
    method: "DELETE",
  });
  if (res.data) {
    clearAuth();
    return true;
  }
  toast.error(getT()("auth.toast.deleteFailed"), { description: res.error.error || undefined });
  return false;
}

export async function unlinkProvider(provider: string): Promise<boolean> {
  const result = await apiFetch(`${API_URL}/v1/auth/identities/${provider}`, okResponseSchema, {
    method: "DELETE",
  });
  if (result.data) {
    await fetchMe();
    return true;
  }
  toast.error(getT()("auth.toast.unlinkFailed"), { description: result.error.error || undefined });
  return false;
}
