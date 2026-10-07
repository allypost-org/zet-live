import type { MapInstance } from "react-map-gl/maplibre";

export function calculateLatOffset(map: MapInstance | null | undefined) {
  const zoom = map?.getZoom() ?? 13;
  return 15 * Math.exp(-0.6 * zoom);
}

/**
 * Bounding box of `points` as `[[west, south], [east, north]]`. A single point
 * yields a zero-area box, which MapLibre resolves to `maxZoom`.
 */
export function boundsFromPoints(
  points: readonly [number, number][],
): [[number, number], [number, number]] | null {
  const first = points[0];
  if (!first) return null;

  let [west, south] = first;
  let [east, north] = first;

  for (const [lng, lat] of points) {
    west = Math.min(west, lng);
    east = Math.max(east, lng);
    south = Math.min(south, lat);
    north = Math.max(north, lat);
  }

  return [
    [west, south],
    [east, north],
  ];
}
