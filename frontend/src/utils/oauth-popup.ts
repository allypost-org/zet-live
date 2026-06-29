import { backendOrigin } from "@/capabilities-store";

const POPUP_WIDTH = 520;
const POPUP_HEIGHT = 680;
const RESPONSE_TIMEOUT = 5 * 60 * 1000;
const POLL_CLOSED_INTERVAL = 500;

export type OAuthPopupResult = {
  ok: boolean;
  provider: string;
  error?: string;
  /** Session bearer token (present on successful login). */
  token?: string;
  /** A link collision: the provider is on another account; needs confirmation. */
  conflict?: boolean;
  /** Single-use token to confirm a transfer (present when `conflict`). */
  transferToken?: string;
};

function centerPopupFeatures(): string {
  const left = Math.max(0, (window.screen.width - POPUP_WIDTH) / 2);
  const top = Math.max(0, (window.screen.height - POPUP_HEIGHT) / 2);
  return `popup=yes,width=${POPUP_WIDTH},height=${POPUP_HEIGHT},left=${left},top=${top}`;
}

type AuthCallbackMessage = {
  type: "zet-auth-callback";
  ok: boolean;
  provider?: string;
  error?: string;
  token?: string;
  conflict?: boolean;
  transferToken?: string;
};

function isAuthCallbackMessage(data: unknown): data is AuthCallbackMessage {
  if (typeof data !== "object" || data === null) return false;
  const msg = data as Record<string, unknown>;
  return msg["type"] === "zet-auth-callback" && typeof msg["ok"] === "boolean";
}

export function openOAuthPopup(
  provider: string,
  link = false,
  ticket?: string,
): Promise<OAuthPopupResult> {
  return new Promise((resolve, reject) => {
    const origin = backendOrigin();
    const openerOrigin = window.location.origin;
    const params = new URLSearchParams({ origin: openerOrigin });
    if (link) {
      params.set("link", "1");
      if (ticket) params.set("ticket", ticket);
    }
    const url = `${origin}/api/v1/auth/${provider}/start?${params.toString()}`;
    const popup = window.open(url, `zet-auth-${provider}`, centerPopupFeatures());

    if (!popup) {
      reject(new Error("Popup blocked. Please allow popups for this site."));
      return;
    }

    const expectedOrigin = origin;

    let settled = false;
    let timeoutId = null as ReturnType<typeof setTimeout> | null;
    let closedPollId = null as ReturnType<typeof setInterval> | null;

    function cleanup() {
      if (timeoutId) clearTimeout(timeoutId);
      if (closedPollId) clearInterval(closedPollId);
      window.removeEventListener("message", onMessage);
    }

    function done(result: OAuthPopupResult) {
      if (settled) return;
      settled = true;
      cleanup();
      try {
        popup?.close();
      } catch {
        // ignore
      }
      resolve(result);
    }

    function onMessage(e: MessageEvent) {
      if (e.origin !== expectedOrigin) return;
      if (!isAuthCallbackMessage(e.data)) return;
      const data = e.data;

      if (data.ok) {
        done({ ok: true, provider: data.provider ?? provider, token: data.token });
      } else if (data.conflict) {
        done({
          ok: false,
          conflict: true,
          provider: data.provider ?? provider,
          transferToken: data.transferToken,
        });
      } else {
        done({ ok: false, provider: data.provider ?? provider, error: data.error });
      }
    }

    window.addEventListener("message", onMessage);

    timeoutId = setTimeout(() => {
      done({ ok: false, provider, error: "Sign-in timed out. Please try again." });
    }, RESPONSE_TIMEOUT);

    closedPollId = setInterval(() => {
      if (popup.closed) {
        done({ ok: false, provider, error: "Sign-in window was closed." });
      }
    }, POLL_CLOSED_INTERVAL);
  });
}
