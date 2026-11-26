import type { StopV1 } from "@/app/entity/v1/stop";

export type StopStation = {
  name: string;
  lat: number;
  lng: number;
  stops: StopV1[];
};

/**
 * Stops with the same GTFS `parent_station` belong to one station.
 * Without a parent, use the ID prefix: ZET stop IDs have the form
 * `<station>_<platform>`, for example `277_21`.
 */
function stationKey(stop: StopV1): string {
  if (stop.parentStation !== null) return `parent:${stop.parentStation}`;
  const separator = stop.id.indexOf("_");
  const prefix = separator === -1 ? stop.id : stop.id.slice(0, separator);
  return `prefix:${prefix}\u0000${stop.name}`;
}

export function groupStopsIntoStations(stops: StopV1[]): StopStation[] {
  const byKey = new Map<string, StopV1[]>();
  for (const stop of stops) {
    const key = stationKey(stop);
    const group = byKey.get(key);
    if (group) {
      group.push(stop);
    } else {
      byKey.set(key, [stop]);
    }
  }

  return [...byKey.values()].map((group) => ({
    name: group[0]!.name,
    lat: group.reduce((acc, stop) => acc + stop.lat, 0) / group.length,
    lng: group.reduce((acc, stop) => acc + stop.lng, 0) / group.length,
    stops: group,
  }));
}
