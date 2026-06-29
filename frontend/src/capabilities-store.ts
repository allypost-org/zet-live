import { create } from "zustand";
import { API_URL } from "@/app/consts";
import type { ProviderPublic } from "@/app/entity/v1/auth";

type CapabilitiesState = {
  providers: ProviderPublic[];
  backendOrigin: string | null;
  loading: boolean;
};

export const capabilitiesStore = create<CapabilitiesState>()(() => ({
  providers: [],
  backendOrigin: null,
  loading: true,
}));

export function useAuthProviders(): ProviderPublic[] {
  return capabilitiesStore((s) => s.providers);
}

export function useCapabilitiesLoading(): boolean {
  return capabilitiesStore((s) => s.loading);
}

export function backendOrigin(): string {
  return (
    capabilitiesStore.getState().backendOrigin ?? new URL(API_URL, window.location.origin).origin
  );
}
