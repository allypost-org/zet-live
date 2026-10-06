import { useEffect } from "react";
import { API_URL } from "@/app/consts";
import { apiFetch } from "@/app/entity/v1/api";
import { capabilitiesSchema } from "@/app/entity/v1/auth";
import { capabilitiesStore } from "@/capabilities-store";
import { featureFlagsStore, setFeatureFlags } from "@/feature-flags-store";
import { authStore } from "@/auth-store";

export function useCapabilities() {
  useEffect(() => {
    let cancelled = false;
    let controller: AbortController | null = null;

    async function load() {
      controller?.abort();
      controller = new AbortController();
      const { signal } = controller;
      const flagsAtStart = featureFlagsStore.getState().flags;
      const token = authStore.getState().token;
      const result = await apiFetch(`${API_URL}/v1/capabilities`, capabilitiesSchema, { signal });
      if (cancelled || signal.aborted || authStore.getState().token !== token) return;
      const data = result.data;
      let backendOrigin = null as string | null;
      if (data?.appUrl) {
        try {
          backendOrigin = new URL(data.appUrl).origin;
        } catch {
          // ignore malformed appUrl
        }
      }
      capabilitiesStore.setState({
        providers: data?.auth.providers ?? [],
        backendOrigin,
        loading: false,
      });
      if (featureFlagsStore.getState().flags === flagsAtStart) {
        setFeatureFlags(data?.featureFlags ?? {});
      }
    }

    const unsubscribe = authStore.subscribe((state, previous) => {
      if (state.token !== previous.token || state.user?.id !== previous.user?.id) {
        setFeatureFlags({});
        void load();
      }
    });
    void load();
    return () => {
      cancelled = true;
      controller?.abort();
      unsubscribe();
    };
  }, []);
}
