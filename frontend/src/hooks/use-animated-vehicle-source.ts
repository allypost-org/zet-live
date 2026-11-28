import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import type { MapRef } from "react-map-gl/maplibre";
import type { GeoJSONSource, Map as MaplibreMap } from "maplibre-gl";
import type { Feature, FeatureCollection, Point } from "geojson";
import {
  appRequestAnimationFrame,
  cancelAnimationOrIdleCallback,
} from "@/utils/polyfill/requestSomeCallback";
import { useSetting, type VehicleMotion } from "@/settings";
import { DEFAULT_MOVE_INTERVAL_MS } from "@/app/vehicle-move-interval";

/**
 * Below this zoom, vehicles move by only a few pixels, so the trail mode is
 * used. This also prevents expensive updates when most vehicles are visible.
 */
const MIN_ANIMATED_ZOOM = 13;

/** A move longer than this is a jump in the data, so the vehicle does not animate. */
const MAX_ANIMATED_MOVE_DEGREES = 0.02;

/**
 * Each `setData` makes MapLibre rebuild the vehicle tiles, which is expensive.
 * The source gets new data only when a visible vehicle moved at least this
 * many screen pixels since the last update.
 */
const MIN_STEP_PIXELS = 2;
/** Minimum time between two updates of the source. */
const MIN_UPDATE_INTERVAL_MS = 1000 / 30;

export type Position = [number, number];

/**
 * A move of one vehicle along `points`, from `from` to `to`, at constant speed.
 * Times use `performance.now()`.
 */
export type Track = {
  from: Position;
  to: Position;
  /** All points of the move, from `from` to `to`. */
  points: Position[];
  /** Distance from `from` to each point of `points`, in degrees of latitude. */
  distances: number[];
  start: number;
  duration: number;
};

/** How a vehicle moves to its new position. */
export type VehicleMove = {
  /** Expected time until the next position, in ms. */
  duration: number;
  /** The previous position that the server sent for the vehicle. */
  from: Position | null;
  /** Points of the route between `from` and the new position. */
  via: Position[];
};

export type VehicleFeatureCollection = FeatureCollection<Point, { id: string }>;

function distance(a: Position, b: Position): number {
  const lngScale = Math.cos((a[1] * Math.PI) / 180);
  return Math.hypot((b[0] - a[0]) * lngScale, b[1] - a[1]);
}

export function makeTrack(points: Position[], start: number, duration: number): Track {
  const distances = [0];
  for (let i = 1; i < points.length; i++) {
    distances.push(distances[i - 1]! + distance(points[i - 1]!, points[i]!));
  }
  return {
    from: points[0]!,
    to: points[points.length - 1]!,
    points,
    distances,
    start,
    duration,
  };
}

function stillTrack(position: Position): Track {
  return makeTrack([position], 0, 0);
}

export function isMoving(track: Track, now: number): boolean {
  return now - track.start < track.duration;
}

/** Distance along the track at `now`, in the units of `track.distances`. */
function distanceAt(track: Track, now: number): number {
  const total = track.distances[track.distances.length - 1]!;
  if (!isMoving(track, now)) return total;
  return (total * (now - track.start)) / track.duration;
}

export function positionAt(track: Track, now: number): Position {
  if (!isMoving(track, now)) return track.to;
  const along = distanceAt(track, now);
  const { points, distances } = track;
  let i = 1;
  while (i < points.length - 1 && distances[i]! < along) i++;
  const from = points[i - 1]!;
  const to = points[i]!;
  const length = distances[i]! - distances[i - 1]!;
  const t = length > 0 ? (along - distances[i - 1]!) / length : 1;
  return [from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t];
}

/** The points of a moving track after its position at `now`. */
function remainingPoints(track: Track, now: number): Position[] {
  if (!isMoving(track, now)) return [];
  const along = distanceAt(track, now);
  return track.points.filter((_, i) => track.distances[i]! > along);
}

function samePosition(a: Position, b: Position): boolean {
  return a[0] === b[0] && a[1] === b[1];
}

function shownIsStale(
  tracks: ReadonlyMap<string, Track>,
  shown: ReadonlyMap<string, Position>,
): boolean {
  for (const [id, track] of tracks) {
    const last = shown.get(id);
    if (!last || !samePosition(last, track.to)) return true;
  }
  return false;
}

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

function prefersReducedMotion(): boolean {
  return window.matchMedia?.(REDUCED_MOTION_QUERY).matches ?? false;
}

function subscribeReducedMotion(onChange: () => void): () => void {
  const query = window.matchMedia?.(REDUCED_MOTION_QUERY);
  query?.addEventListener("change", onChange);
  return () => {
    query?.removeEventListener("change", onChange);
  };
}

/**
 * Returns the vehicle motion to use now. The `vehicleMotion` setting selects
 * animation, but reduced motion and a zoom below `MIN_ANIMATED_ZOOM` use the
 * trail mode.
 */
export function useVehicleMotion(zoom: number): VehicleMotion {
  const setting = useSetting("vehicleMotion");
  const reducedMotion = useSyncExternalStore(subscribeReducedMotion, prefersReducedMotion);
  return setting === "animate" && !reducedMotion && zoom >= MIN_ANIMATED_ZOOM ? "animate" : "trail";
}

/**
 * Returns the largest screen distance in pixels between `shown` and the
 * current position of a moving vehicle in the viewport.
 */
function largestVisibleStep(
  map: MaplibreMap,
  tracks: ReadonlyMap<string, Track>,
  shown: ReadonlyMap<string, Position>,
  now: number,
): number {
  const bounds = map.getBounds();
  // Web Mercator at 512 px tiles. The latitude scale uses the map centre.
  const pixelsPerDegree = (512 * 2 ** map.getZoom()) / 360;
  const latScale = 1 / Math.cos((map.getCenter().lat * Math.PI) / 180);
  let largest = 0;
  for (const [id, track] of tracks) {
    if (!isMoving(track, now)) continue;
    const position = positionAt(track, now);
    if (!bounds.contains(position)) continue;
    const last = shown.get(id);
    if (!last) return Number.POSITIVE_INFINITY;
    const dx = (position[0] - last[0]) * pixelsPerDegree;
    const dy = (position[1] - last[1]) * pixelsPerDegree * latScale;
    largest = Math.max(largest, Math.hypot(dx, dy));
  }
  return largest;
}

/**
 * Sets `data` on the GeoJSON source `sourceId`. With `motion` "animate", a
 * vehicle with a new position moves from its shown position to the new one,
 * through the route points in `moves`. The move takes the duration in `moves`,
 * so that it ends when the next position arrives. With "trail", vehicles jump to the new position.
 * Features are matched by `properties.id`. Returns a function that gives the
 * current track of a vehicle.
 */
export function useAnimatedVehicleSource(
  mapRef: { current: MapRef | null },
  sourceId: string,
  data: VehicleFeatureCollection,
  moves: ReadonlyMap<string, VehicleMove>,
  ready: boolean,
  motion: VehicleMotion,
) {
  const tracks = useRef(new Map<string, Track>());
  const frameId = useRef<number | null>(null);
  /** Positions in the last data given to the source. */
  const shown = useRef(new Map<string, Position>());
  const lastUpdate = useRef(0);

  useEffect(() => {
    if (!ready) return;

    const now = performance.now();
    const animate = motion === "animate" && document.visibilityState === "visible";
    const next = new Map<string, Track>();
    for (const feature of data.features) {
      const id = feature.properties.id;
      const target = feature.geometry.coordinates as Position;
      const existing = tracks.current.get(id);
      // A change to the trail mode ends the moves at their targets.
      if (animate && existing && samePosition(existing.to, target)) {
        next.set(id, existing);
        continue;
      }
      const current = existing ? positionAt(existing, now) : target;
      const jump =
        Math.abs(current[0] - target[0]) > MAX_ANIMATED_MOVE_DEGREES ||
        Math.abs(current[1] - target[1]) > MAX_ANIMATED_MOVE_DEGREES;
      if (!animate || jump) {
        next.set(id, stillTrack(target));
        continue;
      }
      const move = moves.get(id);
      // The route points start at the previous position from the server. They
      // are used only if the vehicle moves from there.
      const via =
        existing && move?.from && samePosition(existing.to, move.from)
          ? [...remainingPoints(existing, now), ...move.via]
          : [];
      next.set(
        id,
        makeTrack([current, ...via, target], now, move?.duration ?? DEFAULT_MOVE_INTERVAL_MS),
      );
    }
    tracks.current = next;

    const push = (source: GeoJSONSource, frameTime: number) => {
      const positions = new Map<string, Position>();
      const features = data.features.map((feature): Feature<Point, { id: string }> => {
        const track = tracks.current.get(feature.properties.id);
        if (!track) return feature;
        const position = positionAt(track, frameTime);
        positions.set(feature.properties.id, position);
        return { ...feature, geometry: { type: "Point", coordinates: position } };
      });
      source.setData({ ...data, features });
      shown.current = positions;
      lastUpdate.current = frameTime;
    };

    // The first frame always updates the source, because other properties of
    // the features can change too.
    let force = true;
    const render = () => {
      frameId.current = null;
      const map = mapRef.current?.getMap();
      const source = map?.getSource<GeoJSONSource>(sourceId);
      if (!map || !source) {
        frameId.current = appRequestAnimationFrame(render);
        return;
      }
      const frameTime = performance.now();
      let moving = false;
      for (const track of tracks.current.values()) {
        if (isMoving(track, frameTime)) {
          moving = true;
          break;
        }
      }
      if (!moving) {
        // Move all vehicles, also the ones outside the viewport, to their targets.
        if (force || shownIsStale(tracks.current, shown.current)) push(source, frameTime);
        return;
      }
      if (
        force ||
        (frameTime - lastUpdate.current >= MIN_UPDATE_INTERVAL_MS &&
          largestVisibleStep(map, tracks.current, shown.current, frameTime) >= MIN_STEP_PIXELS)
      ) {
        push(source, frameTime);
        force = false;
      }
      frameId.current = appRequestAnimationFrame(render);
    };

    cancelAnimationOrIdleCallback(frameId.current);
    frameId.current = appRequestAnimationFrame(render);
    return () => {
      cancelAnimationOrIdleCallback(frameId.current);
      frameId.current = null;
    };
  }, [mapRef, sourceId, data, moves, ready, motion]);

  return useCallback((id: string): Track | undefined => tracks.current.get(id), []);
}
