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

const STOP_DEPARTURES_FLAG = "stop_departures";

/** Non-hook check for fetch paths (workers, store subscriptions). */
export function stopDeparturesEnabled(): boolean {
  return featureFlagsStore.getState().flags[STOP_DEPARTURES_FLAG] === true;
}
