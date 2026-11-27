import { useEffect, useRef } from "react";
import type { MapRef } from "react-map-gl/maplibre";
import type { Map as MaplibreMap } from "maplibre-gl";
import { useStore } from "@/store";
import { calculateLatOffset } from "@/utils/map";
import type { VehicleMotion } from "@/settings";
import {
  isMoving,
  positionAt,
  type Position,
  type Track,
} from "@/hooks/use-animated-vehicle-source";

/** Duration of the first move of the map to a vehicle that the map starts to follow. */
const CATCH_UP_MS = 500;
/** Duration of a map move to a vehicle that does not animate. */
const JUMP_MS = 500;

type FollowState = { id: string; catchUpUntil: number };

function currentTrack(id: string, getTrack: (id: string) => Track | undefined): Track | null {
  const track = getTrack(id);
  if (track) return track;
  const vehicle = useStore.getState().vehicles.get(`vehicle-${id}`);
  if (!vehicle) return null;
  const position: Position = [vehicle.lng, vehicle.lat];
  return { from: position, to: position, start: 0, duration: 0 };
}

function mapCenter(map: MaplibreMap, [lng, lat]: Position): Position {
  return [lng, lat - calculateLatOffset(map)];
}

/**
 * Moves the map with the vehicle until its current track ends. A linear move
 * keeps the vehicle at the same screen position.
 */
function followTrack(map: MaplibreMap, track: Track) {
  const now = performance.now();
  const center = mapCenter(map, track.to);
  if (isMoving(track, now)) {
    map.easeTo({ center, duration: track.start + track.duration - now, easing: (t) => t });
  } else {
    map.easeTo({ center, duration: JUMP_MS });
  }
}

/**
 * Keeps the vehicle `vehicleId` in the map centre. When the map starts to
 * follow a vehicle, it first moves to the vehicle in `CATCH_UP_MS`. After that,
 * it moves together with the shown vehicle. Call it after
 * `useAnimatedVehicleSource`, so that `getTrack` gives the latest tracks.
 */
export function useFollowVehicle(
  mapRef: { current: MapRef | null },
  vehicleId: string | null,
  getTrack: (id: string) => Track | undefined,
  motion: VehicleMotion,
) {
  const vehicles = useStore((s) => s.vehicles);
  const follow = useRef<FollowState | null>(null);

  useEffect(() => {
    if (vehicleId === null) {
      follow.current = null;
      return;
    }
    const map = mapRef.current?.getMap();
    if (!map) return;
    const track = currentTrack(vehicleId, getTrack);
    if (!track) return;

    const now = performance.now();
    const state = follow.current;
    if (state?.id === vehicleId) {
      // The catch-up move continues with the latest track when it ends.
      if (now < state.catchUpUntil) return;
      followTrack(map, track);
      return;
    }

    const started: FollowState = { id: vehicleId, catchUpUntil: now + CATCH_UP_MS };
    follow.current = started;
    // Aim at the place where the shown vehicle will be when the catch-up move ends.
    map.easeTo({
      center: mapCenter(map, positionAt(track, started.catchUpUntil)),
      duration: CATCH_UP_MS,
    });
    // A user move that interrupts the catch-up also disables the follow mode,
    // so the follow state changes and the handler does nothing.
    void map.once("moveend", () => {
      if (follow.current !== started) return;
      started.catchUpUntil = 0;
      const latest = currentTrack(vehicleId, getTrack);
      if (latest) followTrack(map, latest);
    });
  }, [mapRef, vehicleId, getTrack, vehicles, motion]);
}
