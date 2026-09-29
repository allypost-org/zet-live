import { create } from "zustand";

type FeatureFlagsState = {
  /** Only flags the backend reports as enabled for the current user. */
  flags: Record<string, boolean>;
};

export const featureFlagsStore = create<FeatureFlagsState>()(() => ({
  flags: {},
}));

export function setFeatureFlags(flags: Record<string, boolean>): void {
  featureFlagsStore.setState({ flags });
}

/** Whether `key` is enabled for the current user. Fail-closed: unknown/absent = false. */
export function useFeatureFlag(key: string): boolean {
  return featureFlagsStore((s) => s.flags[key] ?? false);
}
