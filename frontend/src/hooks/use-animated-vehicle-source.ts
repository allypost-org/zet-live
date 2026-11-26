import { useEffect, useRef, useSyncExternalStore } from "react";
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

type Position = [number, number];

type Track = {
  from: Position;
  to: Position;
  start: number;
  duration: number;
};

const STILL = { start: 0, duration: 0 };

export type VehicleFeatureCollection = FeatureCollection<Point, { id: string }>;

function isMoving(track: Track, now: number): boolean {
  return now - track.start < track.duration;
}

function positionAt(track: Track, now: number): Position {
  if (!isMoving(track, now)) return track.to;
  const t = (now - track.start) / track.duration;
  return [
    track.from[0] + (track.to[0] - track.from[0]) * t,
    track.from[1] + (track.to[1] - track.from[1]) * t,
  ];
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
 * vehicle with a new position moves from its shown position to the new one.
 * The move takes the vehicle's value in `moveIntervals`, so that it ends when
 * the next position arrives. With "trail", vehicles jump to the new position.
 * Features are matched by `properties.id`.
 */
export function useAnimatedVehicleSource(
  mapRef: { current: MapRef | null },
  sourceId: string,
  data: VehicleFeatureCollection,
  moveIntervals: ReadonlyMap<string, number>,
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
      const target = feature.geometry.coordinates as Position;
      const existing = tracks.current.get(feature.properties.id);
      // A change to the trail mode ends the moves at their targets.
      if (animate && existing && samePosition(existing.to, target)) {
        next.set(feature.properties.id, existing);
        continue;
      }
      const current = existing ? positionAt(existing, now) : target;
      const jump =
        Math.abs(current[0] - target[0]) > MAX_ANIMATED_MOVE_DEGREES ||
        Math.abs(current[1] - target[1]) > MAX_ANIMATED_MOVE_DEGREES;
      next.set(
        feature.properties.id,
        animate && !jump
          ? {
              from: current,
              to: target,
              start: now,
              duration: moveIntervals.get(feature.properties.id) ?? DEFAULT_MOVE_INTERVAL_MS,
            }
          : { from: target, to: target, ...STILL },
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
  }, [mapRef, sourceId, data, moveIntervals, ready, motion]);
}
