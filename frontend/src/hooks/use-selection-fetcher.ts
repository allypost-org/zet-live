import { useEffect } from "react";
import { useStore } from "@/store";
import { fetchFollowingRoute, fetchStopDepartures, fetchStopTrips } from "@/hooks/use-stops";
import { useFeatureFlag } from "@/feature-flags-store";

/**
 * Reactively fetches the data needed for the current selection.
 * Re-runs when the departure-board flag changes so a deep-linked selection
 * re-fetches once flags arrive.
 */
export function useSelectionFetcher() {
  const selection = useStore((s) => s.selection);
  const stopBoard = useFeatureFlag("stop_departures");

  useEffect(() => {
    if (!selection) return;
    switch (selection.type) {
      case "vehicle":
        if (selection.tripId) void fetchFollowingRoute(selection.tripId);
        break;
      case "stop":
        if (stopBoard) void fetchStopDepartures(selection.ids);
        else void fetchStopTrips(selection.ids);
        break;
      case "gbfs-station":
        break;
    }
  }, [selection, stopBoard]);
}
