import { useEffect } from "react";
import { API_URL, FRONTEND_VERSION_ID } from "@/app/consts";
import { frontendVersionIdSchema, versionResponseSchema } from "@/app/entity/v1/version";
import { toast } from "sonner";

const CHECK_INTERVAL = 60 * 1000;
const AUTO_REFRESH_DELAY = 15 * 1000;
const REQUEST_TIMEOUT = 10 * 1000;

const REFRESH_ATTEMPT_KEY = "zet-frontend-refresh-attempt";
const UPDATE_TOAST_ID = "frontend-version-update";

async function fetchVersionId(signal: AbortSignal): Promise<string | null> {
  const controller = new AbortController();
  const abort = () => {
    controller.abort();
  };
  signal.addEventListener("abort", abort, { once: true });
  const timeout = setTimeout(abort, REQUEST_TIMEOUT);
  try {
    if (signal.aborted) return null;
    const response = await fetch(`${API_URL}/v1/version`, {
      cache: "no-store",
      signal: controller.signal,
    });
    if (!response.ok) return null;
    const data = versionResponseSchema.safeParse(await response.json());
    return data.success ? data.data.frontendId : null;
  } catch {
    return null;
  } finally {
    clearTimeout(timeout);
    signal.removeEventListener("abort", abort);
  }
}

export function useVersionCheck() {
  useEffect(() => {
    if (FRONTEND_VERSION_ID === null) return;
    const frontendId = FRONTEND_VERSION_ID;

    const controller = new AbortController();
    let active = true;
    let checking = false;
    let pendingVersionId: string | null = null;
    let dismissedVersionId: string | null = null;
    let reloadTimer: ReturnType<typeof setTimeout> | null = null;

    function clearReloadTimer() {
      if (reloadTimer !== null) {
        clearTimeout(reloadTimer);
        reloadTimer = null;
      }
    }

    async function checkVersion() {
      if (checking) return;
      checking = true;
      const serverId = await fetchVersionId(controller.signal);
      checking = false;
      if (!active || serverId === null) return;

      if (serverId === frontendId) {
        clearReloadTimer();
        pendingVersionId = null;
        toast.dismiss(UPDATE_TOAST_ID);
        try {
          window.sessionStorage.removeItem(REFRESH_ATTEMPT_KEY);
        } catch {
          // Storage can be unavailable in restricted browser contexts.
        }
        return;
      }

      if (serverId === dismissedVersionId) return;
      if (serverId === pendingVersionId) return;

      clearReloadTimer();
      pendingVersionId = serverId;
      let autoRefresh = false;
      try {
        const stored = window.sessionStorage.getItem(REFRESH_ATTEMPT_KEY);
        const attempt = frontendVersionIdSchema.safeParse(stored);
        autoRefresh = !attempt.success || attempt.data !== frontendId;
      } catch {
        // Automatic reloads need persistent state to prevent a reload loop.
      }

      function refreshPage(automatic: boolean) {
        if (!active) return;
        clearReloadTimer();
        try {
          window.sessionStorage.setItem(REFRESH_ATTEMPT_KEY, frontendId);
        } catch {
          if (automatic) {
            showUpdateToast(false);
            return;
          }
        }
        window.location.reload();
      }

      if (autoRefresh) {
        reloadTimer = setTimeout(() => {
          refreshPage(true);
        }, AUTO_REFRESH_DELAY);
      }

      function showUpdateToast(automatic: boolean) {
        toast.info("New version available", {
          id: UPDATE_TOAST_ID,
          description: automatic
            ? "The page will refresh automatically in 15 seconds."
            : "Refresh the page to load the latest version.",
          duration: automatic ? AUTO_REFRESH_DELAY + 1000 : Infinity,
          action: {
            label: "Refresh now",
            onClick: () => {
              refreshPage(false);
            },
          },
          onDismiss: () => {
            if (!active || pendingVersionId !== serverId) return;
            clearReloadTimer();
            pendingVersionId = null;
            dismissedVersionId = serverId;
          },
        });
      }
      showUpdateToast(autoRefresh);
    }

    void checkVersion();
    const intervalId = setInterval(() => void checkVersion(), CHECK_INTERVAL);

    return () => {
      active = false;
      controller.abort();
      clearInterval(intervalId);
      clearReloadTimer();
      toast.dismiss(UPDATE_TOAST_ID);
    };
  }, []);
}
