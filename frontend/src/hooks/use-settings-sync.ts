import { useEffect, useRef, type RefObject } from "react";
import { API_URL } from "@/app/consts";
import { apiFetch } from "@/app/entity/v1/api";
import { authStore, useAuthStatus } from "@/auth-store";
import {
  applySyncSettings,
  getSyncSettings,
  serializeSync,
  settingsStore,
  syncSettingsSchema,
} from "@/settings";
import { appQueueMicrotask } from "@/utils/polyfill/requestSomeCallback";

const PUSH_DEBOUNCE_MS = 500;

async function pullSettings(): Promise<"ok" | "missing" | "error"> {
  const res = await apiFetch(`${API_URL}/v1/settings`, syncSettingsSchema);
  if (res.data) {
    applySyncSettings(res.data);
    return "ok";
  }
  if (res.error.status === 404) return "missing";
  return "error";
}

async function pushSettings(lastPushedRef: RefObject<string>): Promise<void> {
  const snapshot = JSON.stringify(getSyncSettings());
  const res = await apiFetch(`${API_URL}/v1/settings`, syncSettingsSchema, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: snapshot,
  });
  if (res.error) {
    console.error("Failed to push settings", res.error.error);
    return;
  }
  lastPushedRef.current = snapshot;
}

export function useSettingsSync() {
  const status = useAuthStatus();
  const applyingRef = useRef(false);
  const lastPushedRef = useRef(JSON.stringify(getSyncSettings()));

  useEffect(() => {
    if (status !== "authenticated") return;
    let cancelled = false;

    void (async () => {
      applyingRef.current = true;
      const result = await pullSettings();
      if (cancelled || authStore.getState().status !== "authenticated") {
        applyingRef.current = false;
        return;
      }
      lastPushedRef.current = JSON.stringify(getSyncSettings());
      if (result === "missing") {
        await pushSettings(lastPushedRef);
      }
      appQueueMicrotask(() => {
        applyingRef.current = false;
      });
    })();

    return () => {
      cancelled = true;
    };
  }, [status]);

  useEffect(() => {
    let timer = null as ReturnType<typeof setTimeout> | null;

    const unsub = settingsStore.subscribe((state) => {
      if (applyingRef.current) return;
      if (authStore.getState().status !== "authenticated") return;

      const snapshot = JSON.stringify(serializeSync(state));
      if (snapshot === lastPushedRef.current) return;

      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        void pushSettings(lastPushedRef);
      }, PUSH_DEBOUNCE_MS);
    });

    return () => {
      unsub();
      if (timer) clearTimeout(timer);
    };
  }, []);
}
