import { z } from "zod";

/** A `[lng, lat]` position. */
export type LngLat = [number, number];

/** Maximum path points in a vehicle update. The backend uses the same limit. */
const MAX_PATH_POINTS = 256;

function optionalPath(value: unknown): LngLat[] {
  if (!Array.isArray(value) || value.length > MAX_PATH_POINTS * 2 || value.length % 2 !== 0) {
    return [];
  }
  const points: LngLat[] = [];
  for (let i = 0; i < value.length; i += 2) {
    const lng: unknown = value[i];
    const lat: unknown = value[i + 1];
    if (typeof lng !== "number" || typeof lat !== "number") return [];
    if (!Number.isFinite(lng) || !Number.isFinite(lat)) return [];
    if (Math.abs(lng) > 180 || Math.abs(lat) > 90) return [];
    points.push([lng, lat]);
  }
  return points;
}

function optionalString(value: unknown): string | null {
  return typeof value === "string" || typeof value === "number" ? String(value) : null;
}

function optionalNumber(value: unknown): number | null {
  if (typeof value !== "number" && typeof value !== "string") return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

/** Fields match `Vehicle::to_simple()` in the backend. */
export const simpleVehicleSchema = z
  .tuple([z.string(), z.string(), z.string(), z.number(), z.number()])
  .rest(z.unknown())
  .transform(
    ([
      id,
      routeId,
      tripId,
      latitude,
      longitude,
      prevLat,
      prevLng,
      nextStopId,
      nextStopSequence,
      nextStopArrivalDelay,
      nextStopArrivalTime,
      bearing,
      routeLongName,
      tripHeadsign,
      path,
    ]) => ({
      id,
      routeId,
      tripId,
      latitude,
      longitude,
      prevLat: optionalNumber(prevLat),
      prevLng: optionalNumber(prevLng),
      nextStopId: optionalString(nextStopId),
      nextStopSequence: optionalNumber(nextStopSequence),
      nextStopArrivalDelay: optionalNumber(nextStopArrivalDelay),
      nextStopArrivalTime: optionalNumber(nextStopArrivalTime),
      bearing: optionalNumber(bearing),
      routeLongName: optionalString(routeLongName),
      tripHeadsign: optionalString(tripHeadsign),
      path: optionalPath(path),
    }),
  );

export type VehicleData = z.infer<typeof simpleVehicleSchema>;
