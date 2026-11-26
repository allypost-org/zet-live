import { useRef, useEffect, useCallback } from "react";
import { useStore } from "@/store";

const DEBOUNCE_MS = 300;

function buildUrlParams(): string {
  const { selection, vehicleSelection } = useStore.getState();
  const params = new URLSearchParams();

  if (selection?.type === "vehicle") {
    params.set("vehicle", selection.id);
    if (selection.tripId) params.set("trip", selection.tripId);
    if (vehicleSelection?.followEnabled) params.set("follow", "1");
  } else if (selection?.type === "stop") {
    for (const id of selection.ids) {
      params.append("stop", id);
    }
  } else if (selection?.type === "gbfs-station") {
    params.set("station", selection.id);
  }

  return params.toString();
}

function applyUrlParams(params: URLSearchParams) {
  const vehicleParam = params.get("vehicle");
  const tripParam = params.get("trip");
  const followParam = params.get("follow");
  const stopParams = params.getAll("stop");
  const stationParam = params.get("station");

  if (vehicleParam) {
    useStore.getState().selectVehicle(vehicleParam, tripParam ?? "");
    useStore.getState().setFollowEnabled(followParam === "1");
  } else if (stopParams.length > 0) {
    useStore.getState().selectStop(stopParams);
  } else if (stationParam) {
    useStore.getState().selectGbfsStation(stationParam);
  } else {
    useStore.getState().clearSelection();
  }
}

export function useUrlSync() {
  const isRestoringRef = useRef(false);
  const initializedRef = useRef(false);
  const debounceRef = useRef<number | null>(null);

  const selection = useStore((s) => s.selection);
  const followEnabled = useStore((s) => s.vehicleSelection?.followEnabled ?? false);

  const syncUrl = useCallback((push: boolean) => {
    const newSearch = buildUrlParams();
    const targetPath = newSearch
      ? `${location.pathname}?${newSearch}${location.hash}`
      : `${location.pathname}${location.hash}`;
    if (push) {
      history.pushState(null, "", targetPath);
    } else {
      history.replaceState(null, "", targetPath);
    }
  }, []);

  useEffect(() => {
    if (!initializedRef.current) return;
    if (isRestoringRef.current) return;

    if (debounceRef.current !== null) {
      clearTimeout(debounceRef.current);
    }

    syncUrl(false);

    debounceRef.current = window.setTimeout(() => {
      debounceRef.current = null;
      syncUrl(true);
    }, DEBOUNCE_MS);
  }, [selection, followEnabled, syncUrl]);

  useEffect(() => {
    return () => {
      if (debounceRef.current !== null) {
        clearTimeout(debounceRef.current);
      }
    };
  }, []);

  const handlePopState = useCallback(() => {
    isRestoringRef.current = true;
    applyUrlParams(new URLSearchParams(location.search));
    isRestoringRef.current = false;
  }, []);

  useEffect(() => {
    window.addEventListener("popstate", handlePopState);
    return () => {
      window.removeEventListener("popstate", handlePopState);
    };
  }, [handlePopState]);

  const simpleStops = useStore((s) => s.simpleStops);
  const gbfsStations = useStore((s) => s.gbfsStations);

  useEffect(() => {
    if (initializedRef.current) return;

    const params = new URLSearchParams(location.search);
    const vehicleParam = params.get("vehicle");
    const tripParam = params.get("trip");
    const stopParams = params.getAll("stop");
    const stationParam = params.get("station");

    const hasStops = Object.keys(simpleStops).length > 0;
    const hasStations = gbfsStations.size > 0;

    if (vehicleParam) {
      if (!hasStops) return;
      const follow = params.get("follow") === "1";
      isRestoringRef.current = true;
      useStore.getState().selectVehicle(vehicleParam, tripParam ?? "");
      useStore.getState().setFollowEnabled(follow);
      isRestoringRef.current = false;
    } else if (stopParams.length > 0) {
      if (!hasStops) return;
      isRestoringRef.current = true;
      useStore.getState().selectStop(stopParams);
      isRestoringRef.current = false;
    } else if (stationParam) {
      if (!hasStations) return;
      isRestoringRef.current = true;
      useStore.getState().selectGbfsStation(stationParam);
      isRestoringRef.current = false;
    }

    initializedRef.current = true;
  }, [simpleStops, gbfsStations]);
}
