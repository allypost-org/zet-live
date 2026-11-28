import { useEffect } from "react";
import type { MapRef } from "react-map-gl/maplibre";
import type { Map as MaplibreMap } from "maplibre-gl";
import { useStore } from "@/store";
import { calculateLatOffset } from "@/utils/map";
import {
  appRequestAnimationFrame,
  cancelAnimationOrIdleCallback,
} from "@/utils/polyfill/requestSomeCallback";
import {
  isMoving,
  makeTrack,
  positionAt,
  type Position,
  type Track,
} from "@/hooks/use-animated-vehicle-source";

/** Duration of the first move of the map to a vehicle that the map starts to follow. */
const CATCH_UP_MS = 500;
/** Duration of a map move to a vehicle that does not animate, or that jumped. */
const JUMP_MS = 500;
/**
 * Minimum time between two map moves. A continuous map animation draws the
 * full map on each display frame, which uses much CPU on high refresh rates.
 */
const FOLLOW_INTERVAL_MS = 1000 / 30;
/** A smaller move of the vehicle on the screen does not move the map. */
const MIN_FOLLOW_STEP_PIXELS = 0.5;
/** A larger move of the vehicle on the screen is a jump, so the map eases to it. */
const MAX_FOLLOW_STEP_PIXELS = 100;

function currentTrack(id: string, getTrack: (id: string) => Track | undefined): Track | null {
  const track = getTrack(id);
  if (track) return track;
  const vehicle = useStore.getState().vehicles.get(`vehicle-${id}`);
  if (!vehicle) return null;
  const position: Position = [vehicle.lng, vehicle.lat];
  return makeTrack([position], 0, 0);
}

function mapCenter(map: MaplibreMap, [lng, lat]: Position): Position {
  return [lng, lat - calculateLatOffset(map)];
}

/**
 * Keeps the vehicle `vehicleId` in the map centre. When the map starts to
 * follow a vehicle, it first moves to the vehicle in `CATCH_UP_MS`. After that,
 * it moves with the shown vehicle at most once per `FOLLOW_INTERVAL_MS`.
 * `getTrack` gives the current track of a vehicle.
 *
 * `jumpTo` stops user gestures, so the map does not move while it moves for
 * another reason, for example a user zoom or the catch-up move.
 */
export function useFollowVehicle(
  mapRef: { current: MapRef | null },
  vehicleId: string | null,
  getTrack: (id: string) => Track | undefined,
) {
  useEffect(() => {
    if (vehicleId === null) return;

    let frameId: number | null = null;
    let caughtUp = false;
    let lastMove = 0;
    const tick = () => {
      frameId = appRequestAnimationFrame(tick);
      const map = mapRef.current?.getMap();
      const now = performance.now();
      if (!map || now - lastMove < FOLLOW_INTERVAL_MS || map.isMoving()) return;
      // The map or the vehicle can arrive after the follow mode starts.
      const track = currentTrack(vehicleId, getTrack);
      if (!track) return;
      lastMove = now;
      if (!caughtUp) {
        caughtUp = true;
        // Aim at the place where the shown vehicle will be when the catch-up move ends.
        map.easeTo({
          center: mapCenter(map, positionAt(track, now + CATCH_UP_MS)),
          duration: CATCH_UP_MS,
        });
        return;
      }
      const center = mapCenter(map, positionAt(track, now));
      const from = map.project(map.getCenter());
      const to = map.project(center);
      const step = Math.hypot(to.x - from.x, to.y - from.y);
      if (step < MIN_FOLLOW_STEP_PIXELS) return;
      if (isMoving(track, now) && step <= MAX_FOLLOW_STEP_PIXELS) {
        map.jumpTo({ center });
      } else {
        map.easeTo({ center, duration: JUMP_MS });
      }
    };
    frameId = appRequestAnimationFrame(tick);
    return () => {
      cancelAnimationOrIdleCallback(frameId);
    };
  }, [mapRef, vehicleId, getTrack]);
}
