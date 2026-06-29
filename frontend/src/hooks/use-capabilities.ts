import { useEffect } from "react";
import { API_URL } from "@/app/consts";
import { apiFetch } from "@/app/entity/v1/api";
import { capabilitiesSchema } from "@/app/entity/v1/auth";
import { capabilitiesStore } from "@/capabilities-store";

export function useCapabilities() {
  useEffect(() => {
    let cancelled = false;

    async function load() {
      const result = await apiFetch(`${API_URL}/v1/capabilities`, capabilitiesSchema);
      if (cancelled) return;
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
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, []);
}
