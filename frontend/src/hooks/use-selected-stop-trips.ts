import { useMemo } from "react";
import { useStore } from "@/store";
import { useFeatureFlag } from "@/feature-flags-store";

/**
 * Trips serving the selected stop. With the departure-boards flag on and a boarding
 * location focused, this narrows to that location's live departures. The map highlight
 * and the stop sheet's "Fit all" both read it, so they cannot disagree on which
 * vehicles belong to the stop.
 */
export function useSelectedStopTripIds(): Set<string> | null {
  const stopSelection = useStore((s) => s.stopSelection);
  const stopBoardFlag = useFeatureFlag("stop_departures");

  return useMemo(() => {
    if (!stopSelection) return null;
    const focusedStopId = stopBoardFlag ? stopSelection.focusedStopId : null;
    if (focusedStopId === null) return stopSelection.tripIds;
    const board = stopSelection.departureBoards.find((entry) => entry.stopId === focusedStopId);
    return new Set((board?.departures ?? []).flatMap((d) => (d.kind === "live" ? [d.tripId] : [])));
  }, [stopSelection, stopBoardFlag]);
}
