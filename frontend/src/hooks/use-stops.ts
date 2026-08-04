import type { V1Message } from "@/app/entity/v1/message";
import {
  apiFetch,
  tripInfoResponseSchema,
  tripStopTimesResponseSchema,
  stopArrivalsResponseSchema,
  stopTripsResponseSchema,
} from "@/app/entity/v1/api";
import { VehicleV1 } from "@/app/entity/v1/vehicle";
import { StopV1 } from "@/app/entity/v1/stop";
import { GbfsStationV1 } from "@/app/entity/v1/gbfs-station";
import { API_URL } from "@/app/consts";
import {
  useStore,
  type VehicleLocationPair,
  type VehicleSelection,
  type StopSelection,
  updateMaxBounds,
} from "@/store";
import type { StopsUpdateResponse } from "@/app/entity/shared";
import { buildRouteDisplayedStops, patchTripStopTimesFromVehicle } from "@/app/trip-stop-times";
import { toast } from "sonner";

function vehicleMapKey(id: string) {
  return `vehicle-${id}`;
}

function patchVehicleSelection(patch: Partial<VehicleSelection>) {
  const vs = useStore.getState().vehicleSelection;
  if (!vs) return;
  useStore.setState({ vehicleSelection: { ...vs, ...patch } });
}

function patchStopSelection(patch: Partial<StopSelection>) {
  const ss = useStore.getState().stopSelection;
  if (!ss) return;
  useStore.setState({ stopSelection: { ...ss, ...patch } });
}

function sameStopIds(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((id, i) => id === b[i]);
}

export function handleStopsUpdate(response: StopsUpdateResponse) {
  if (response.stops) {
    const stops = response.stops.map((s) => new StopV1(s));

    useStore.setState({
      simpleStops: Object.fromEntries(stops.map((stop) => [stop.id, stop])),
      ...(response.bounds ? { stopBounds: response.bounds } : {}),
    });
    updateMaxBounds();
  }

  const state = useStore.getState();
  const useGrouped = state.selection === null;
  useStore.setState({
    stopsGrouped: response.grouped,
    displayedStops: useGrouped ? response.grouped : state.displayedStops,
  });
}

export function processMessage(message: V1Message) {
  if (typeof message.d === "object" && "vehicles" in message.d) {
    const rawVehicles = message.d.vehicles;
    const currentMap = useStore.getState().vehicles;
    const locationPairs: VehicleLocationPair[] = [];

    let minLat = 89.5;
    let maxLat = -89.5;
    let minLng = 89.5;
    let maxLng = -89.5;

    const newMap = new Map<string, VehicleV1>();

    for (const raw of rawVehicles) {
      const row = raw as (string | number)[];
      const vehicle = VehicleV1.fromSimple(row);
      const key = vehicle.getMapId();

      const prev = currentMap.get(key) ?? vehicle;
      if (prev.prevLat !== null && prev.prevLng !== null) {
        const isBus = vehicle.routeId.length > 2;
        locationPairs.push({
          from: [prev.prevLng, prev.prevLat],
          to: [vehicle.lng, vehicle.lat],
          color: isBus ? "#00f" : "#f00",
          vehicleType: isBus ? "bus" : "tram",
        });
      }

      newMap.set(key, vehicle);

      minLat = Math.min(minLat, vehicle.lat);
      maxLat = Math.max(maxLat, vehicle.lat);
      minLng = Math.min(minLng, vehicle.lng);
      maxLng = Math.max(maxLng, vehicle.lng);
    }

    const state = useStore.getState();
    const selection = state.selection;
    let vehicleSelection = state.vehicleSelection;

    if (selection?.type === "vehicle" && vehicleSelection) {
      const vehicle = newMap.get(vehicleMapKey(selection.id));
      if (vehicle && vehicle.tripId === selection.tripId) {
        const patched = patchTripStopTimesFromVehicle(
          vehicleSelection.tripStopTimes,
          vehicle.nextStopSequence,
          vehicle.nextStopArrivalTime,
        );
        if (patched !== vehicleSelection.tripStopTimes) {
          vehicleSelection = { ...vehicleSelection, tripStopTimes: patched };
        }
      }
    }

    useStore.setState({
      vehicles: newMap,
      vehicleBounds: [
        [minLng, minLat],
        [maxLng, maxLat],
      ],
      deltaMoveLines: locationPairs,
      ...(vehicleSelection !== state.vehicleSelection ? { vehicleSelection } : {}),
    });
    updateMaxBounds();

    const now = Date.now();
    if (now - lastStopTimesRefresh >= STOP_TIMES_REFRESH_INTERVAL) {
      lastStopTimesRefresh = now;

      const sel = useStore.getState().selection;
      if (sel?.type === "vehicle" && sel.tripId) {
        void refreshTripStopTimes(sel.tripId);
      } else if (sel?.type === "stop") {
        void refreshStopArrivalTimes(sel.ids);
      }
    }
  } else if (typeof message.d === "object" && "gbfsStations" in message.d) {
    handleGbfsStations(message.d.gbfsStations as (string | number)[][]);
  }
}

function recomputeGbfsBounds(
  stations: Map<string, GbfsStationV1>,
): [[number, number], [number, number]] {
  let minLat = 89.5;
  let maxLat = -89.5;
  let minLng = 89.5;
  let maxLng = -89.5;
  let hasAny = false;

  for (const s of stations.values()) {
    hasAny = true;
    minLat = Math.min(minLat, s.lat);
    maxLat = Math.max(maxLat, s.lat);
    minLng = Math.min(minLng, s.lng);
    maxLng = Math.max(maxLng, s.lng);
  }

  if (!hasAny) {
    return [
      [89.5, 89.5],
      [-89.5, -89.5],
    ];
  }

  return [
    [minLng, minLat],
    [maxLng, maxLat],
  ];
}

function handleGbfsStations(raw: (string | number)[][]) {
  const newMap = new Map<string, GbfsStationV1>();
  for (const row of raw) {
    const station = GbfsStationV1.fromSimple(row);
    newMap.set(station.getMapId(), station);
  }

  useStore.setState({
    gbfsStations: newMap,
    gbfsBounds: recomputeGbfsBounds(newMap),
  });
  updateMaxBounds();
}

let followingRouteAbort: AbortController | null = null;
let followingRouteRefreshAbort: AbortController | null = null;

export async function fetchFollowingRoute(tripId: string) {
  followingRouteAbort?.abort();
  followingRouteRefreshAbort?.abort();
  followingRouteAbort = new AbortController();
  const { signal } = followingRouteAbort;

  const isStale = () => {
    const sel = useStore.getState().selection;
    return sel?.type !== "vehicle" || sel.tripId !== tripId;
  };

  const result = await apiFetch(
    `${API_URL}/v1/schedule/trip-info/${tripId}`,
    tripInfoResponseSchema,
    { signal },
  );

  if (signal.aborted || isStale()) return;

  if (result.error) {
    const { error } = result.error;
    const isNotFound = result.error.status === 404;

    if (!isNotFound) {
      toast.error("Failed to load trip info", { description: error });
    }

    const state = useStore.getState();
    const vehicle =
      state.selection?.type === "vehicle"
        ? state.vehicles.get(vehicleMapKey(state.selection.id))
        : null;

    let fallbackStops: { name: string; lat: number; lng: number; ids: string[] }[] = [];

    if (vehicle?.nextStopId) {
      const stop = state.simpleStops[vehicle.nextStopId];
      if (stop) {
        fallbackStops = [{ name: stop.name, lat: stop.lat, lng: stop.lng, ids: [stop.id] }];
      }
    }

    useStore.setState({ displayedStops: fallbackStops });
    patchVehicleSelection({
      route: null,
      tripStopTimes: null,
      fetchError: isNotFound ? "Could not find full stop list.\nThis is a temporary issue." : error,
    });
    return;
  }

  const shape = result.data;
  const simpleStops = useStore.getState().simpleStops;
  const tripStopTimes = shape.d.stopTimes;
  const partialNotice = shape.d.partial
    ? "No published schedule for this trip — showing real-time data only."
    : null;

  useStore.setState({
    displayedStops: buildRouteDisplayedStops(tripStopTimes, simpleStops),
  });
  patchVehicleSelection({
    route: shape.d.route,
    tripStopTimes,
    fetchError: partialNotice,
  });
}

let stopTripsAbort: AbortController | null = null;
let stopTripsRefreshAbort: AbortController | null = null;

let lastStopTimesRefresh = 0;
const STOP_TIMES_REFRESH_INTERVAL = 15_000;

async function refreshTripStopTimes(tripId: string) {
  followingRouteRefreshAbort?.abort();
  followingRouteRefreshAbort = new AbortController();
  const { signal } = followingRouteRefreshAbort;

  const isStale = () => {
    const sel = useStore.getState().selection;
    return sel?.type !== "vehicle" || sel.tripId !== tripId;
  };

  const result = await apiFetch(
    `${API_URL}/v1/schedule/trip-info/${tripId}`,
    tripStopTimesResponseSchema,
    { signal },
  );

  if (signal.aborted || isStale()) return;

  if (result.error) {
    if (result.error.status !== 404) {
      console.error("Failed to refresh trip stop times:", result.error.error);
    }
    return;
  }

  patchVehicleSelection({ tripStopTimes: result.data.d.stopTimes });
}

async function refreshStopArrivalTimes(stopIds: string[]) {
  stopTripsRefreshAbort?.abort();
  stopTripsRefreshAbort = new AbortController();
  const { signal } = stopTripsRefreshAbort;

  const isStale = () => {
    const sel = useStore.getState().selection;
    return sel?.type !== "stop" || !sameStopIds(sel.ids, stopIds);
  };

  const queryParams = new URLSearchParams();
  for (const stopId of stopIds) {
    queryParams.append("stop", stopId);
  }

  const result = await apiFetch(
    `${API_URL}/v1/schedule/stop-trips?${queryParams.toString()}`,
    stopArrivalsResponseSchema,
    { signal },
  );

  if (signal.aborted || isStale()) return;

  if (result.error) {
    if (result.error.status === 404) {
      patchStopSelection({ fetchError: result.error.error });
    } else {
      toast.error("Failed to refresh arrivals", { description: result.error.error });
    }
    return;
  }

  patchStopSelection({ arrivalTimes: result.data.d.arrivalTimes, fetchError: null });
}

export async function fetchStopTrips(stopIds: string[]) {
  stopTripsAbort?.abort();
  stopTripsRefreshAbort?.abort();
  stopTripsAbort = new AbortController();
  const { signal } = stopTripsAbort;

  const isStale = () => {
    const sel = useStore.getState().selection;
    return sel?.type !== "stop" || !sameStopIds(sel.ids, stopIds);
  };

  const queryParams = new URLSearchParams();
  for (const stopId of stopIds) {
    queryParams.append("stop", stopId);
  }

  const result = await apiFetch(
    `${API_URL}/v1/schedule/stop-trips?${queryParams.toString()}`,
    stopTripsResponseSchema,
    { signal },
  );

  if (signal.aborted || isStale()) return;

  if (result.error) {
    if (result.error.status === 404) {
      patchStopSelection({ fetchError: result.error.error });
    } else {
      toast.error("Failed to load stop trips", { description: result.error.error });
    }
    return;
  }

  const tripIds = result.data.d.stopTrips;
  const vehicles = useStore.getState().vehicles;
  const routes = new Set<string>();
  for (const v of vehicles.values()) {
    if (tripIds.includes(v.tripId)) {
      routes.add(v.routeId);
    }
  }
  const sortedRoutes = Array.from(routes).sort((a, b) => {
    const na = parseInt(a, 10);
    const nb = parseInt(b, 10);
    if (!isNaN(na) && !isNaN(nb)) return na - nb;
    return a.localeCompare(b);
  });

  patchStopSelection({
    tripIds: new Set(tripIds),
    routes: sortedRoutes,
    arrivalTimes: result.data.d.arrivalTimes,
    fetchError: null,
  });
}
