import { create } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";
import { z } from "zod";
import type { VehicleV1 } from "./app/entity/v1/vehicle";
import type { StopV1 } from "./app/entity/v1/stop";
import type { GbfsStationV1 } from "./app/entity/v1/gbfs-station";
import type { GroupedStop } from "./app/entity/shared";
import type { TripStopTimeEntry } from "./app/trip-stop-times";
import { stopArrivalTimeSchema } from "./app/entity/v1/api";
import type { GlobalNotice } from "./app/entity/v1/message";

export type { TripStopTimeEntry };

export type { GroupedStop };

export type VehicleLocationPair = {
  from: [number, number];
  to: [number, number];
  color: string;
  vehicleType: "bus" | "tram";
};

export type StopArrivalTime = z.infer<typeof stopArrivalTimeSchema>;

export type Selection =
  | { type: "vehicle"; id: string; tripId: string }
  | { type: "stop"; ids: string[] }
  | { type: "gbfs-station"; id: string };

export type VehicleSelection = {
  route: [number, number][] | null;
  tripStopTimes: TripStopTimeEntry[] | null;
  fetchError: string | null;
  followEnabled: boolean;
};

export type StopSelection = {
  name: string;
  routes: string[];
  tripIds: Set<string> | null;
  arrivalTimes: StopArrivalTime[] | null;
  fetchError: string | null;
};

export type StoreState = {
  vehicles: Map<string, VehicleV1>;
  vehicleBounds: [[number, number], [number, number]];

  simpleStops: Record<string, StopV1>;
  stopsGrouped: GroupedStop[];
  activeStopIds: Set<string>;
  stopBounds: [[number, number], [number, number]];

  gbfsStations: Map<string, GbfsStationV1>;
  gbfsBounds: [[number, number], [number, number]];

  selection: Selection | null;
  vehicleSelection: VehicleSelection | null;
  stopSelection: StopSelection | null;

  displayedStops: GroupedStop[];

  deltaMoveLines: VehicleLocationPair[];

  lastUpdate: number | null;
  lastError: string | null;
  wsConnected: boolean;

  mapReady: boolean;

  maxBounds: [[number, number], [number, number]] | null;

  flyToTarget: { longitude: number; latitude: number } | null;

  searchMatchedVehicleMapIds: Set<string> | null;
  searchMatchedStopIds: Set<string> | null;

  globalNotices: GlobalNotice[] | null;
  userNotices: GlobalNotice[] | null;

  selectVehicle: (id: string, tripId: string, flyTo?: boolean) => void;
  selectStop: (ids: string[]) => void;
  selectGbfsStation: (id: string, flyTo?: boolean) => void;
  clearSelection: () => void;
  setFollowEnabled: (enabled: boolean) => void;
};

const DEFAULT_BOUNDS: [[number, number], [number, number]] = [
  [-89.5, -89.5],
  [89.5, 89.5],
];

function vehicleMapKey(id: string) {
  return `vehicle-${id}`;
}

function gbfsStationMapKey(id: string) {
  return `gbfs-station-${id}`;
}

function resolveStopDisplayName(ids: string[], simpleStops: Record<string, StopV1>): string {
  for (const id of ids) {
    const name = simpleStops[id]?.name;
    if (name) return name;
  }
  return "Unknown stop";
}

function buildStopDisplayedStops(
  ids: string[],
  simpleStops: Record<string, StopV1>,
): GroupedStop[] {
  return ids
    .map((id) => simpleStops[id])
    .filter((stop): stop is StopV1 => Boolean(stop))
    .map((stop) => ({ name: stop.name, lat: stop.lat, lng: stop.lng, ids: [stop.id] }));
}

export const useStore = create<StoreState>()(
  subscribeWithSelector((set, get) => ({
    vehicles: new Map(),
    vehicleBounds: DEFAULT_BOUNDS,

    simpleStops: {},
    stopsGrouped: [],
    activeStopIds: new Set(),
    stopBounds: DEFAULT_BOUNDS,

    gbfsStations: new Map(),
    gbfsBounds: DEFAULT_BOUNDS,

    selection: null,
    vehicleSelection: null,
    stopSelection: null,

    displayedStops: [],

    deltaMoveLines: [],

    lastUpdate: null,
    lastError: null,
    wsConnected: false,

    mapReady: false,

    maxBounds: null,

    flyToTarget: null,

    searchMatchedVehicleMapIds: null,
    searchMatchedStopIds: null,

    globalNotices: null,
    userNotices: null,

    selectVehicle: (id, tripId, flyTo = false) => {
      set({
        selection: { type: "vehicle", id, tripId },
        vehicleSelection: {
          route: null,
          tripStopTimes: null,
          fetchError: null,
          followEnabled: false,
        },
        stopSelection: null,
      });

      if (flyTo) {
        const vehicle = get().vehicles.get(vehicleMapKey(id));
        if (vehicle) {
          set({ flyToTarget: { longitude: vehicle.lng, latitude: vehicle.lat } });
        }
      }
    },

    selectStop: (ids) => {
      const { simpleStops } = get();
      const name = resolveStopDisplayName(ids, simpleStops);

      set({
        selection: { type: "stop", ids },
        stopSelection: {
          name,
          routes: [],
          tripIds: null,
          arrivalTimes: null,
          fetchError: null,
        },
        vehicleSelection: null,
        displayedStops: buildStopDisplayedStops(ids, simpleStops),
      });
    },

    selectGbfsStation: (id, flyTo = false) => {
      const { stopsGrouped } = get();
      set({
        selection: { type: "gbfs-station", id },
        vehicleSelection: null,
        stopSelection: null,
        displayedStops: stopsGrouped,
      });

      if (flyTo) {
        const station = get().gbfsStations.get(gbfsStationMapKey(id));
        if (station) {
          set({ flyToTarget: { longitude: station.lng, latitude: station.lat } });
        }
      }
    },

    clearSelection: () => {
      const { stopsGrouped } = get();
      set({
        selection: null,
        vehicleSelection: null,
        stopSelection: null,
        displayedStops: stopsGrouped,
      });
    },

    setFollowEnabled: (enabled) => {
      const { vehicleSelection } = get();
      if (!vehicleSelection) return;
      set({
        vehicleSelection: {
          ...vehicleSelection,
          followEnabled: enabled,
        },
      });
    },
  })),
);

export function updateMaxBounds() {
  const { stopBounds, vehicleBounds, gbfsBounds } = useStore.getState();
  const pad = 0.05;

  useStore.setState({
    maxBounds: [
      [
        Math.min(stopBounds[0][0], vehicleBounds[0][0], gbfsBounds[0][0]) - pad,
        Math.min(stopBounds[0][1], vehicleBounds[0][1], gbfsBounds[0][1]) - pad,
      ],
      [
        Math.max(stopBounds[1][0], vehicleBounds[1][0], gbfsBounds[1][0]) + pad,
        Math.max(stopBounds[1][1], vehicleBounds[1][1], gbfsBounds[1][1]) + pad,
      ],
    ],
  });
}
