import { v1MessageSchema, type V1Message } from "../app/entity/v1/message";
import type { StopData, GroupedStop, StopsUpdateResponse } from "../app/entity/shared";
import { decode as decodeCbor } from "cbor2";

const DEBUG = (import.meta.env.VITE_DEBUG as string | undefined) === "true";

type StopGroup = {
  stops: StopData[];
  minLat: number;
  maxLat: number;
  minLng: number;
  maxLng: number;
};

const API_URL = (import.meta.env.VITE_API_URL as string | undefined) ?? "/api";

let cachedStops: StopData[] = [];
let cachedActiveStopIds = new Set<string>();
let cachedGroupedAll: GroupedStop[] = [];
let fetchIntervalId: ReturnType<typeof setInterval> | null = null;

function mergedBboxArea(group: StopGroup, stop: StopData) {
  const minLat = Math.min(group.minLat, stop.lat);
  const maxLat = Math.max(group.maxLat, stop.lat);
  const minLng = Math.min(group.minLng, stop.lng);
  const maxLng = Math.max(group.maxLng, stop.lng);
  return (maxLat - minLat) * (maxLng - minLng);
}

function extendGroup(group: StopGroup, stop: StopData) {
  group.stops.push(stop);
  group.minLat = Math.min(group.minLat, stop.lat);
  group.maxLat = Math.max(group.maxLat, stop.lat);
  group.minLng = Math.min(group.minLng, stop.lng);
  group.maxLng = Math.max(group.maxLng, stop.lng);
}

function computeGroupedStops(stops: StopData[], activeStopIds: Set<string> | null): GroupedStop[] {
  const stopsByName: Record<string, StopData[]> = {};
  for (const stop of stops) {
    if (!stopsByName[stop.name]) {
      stopsByName[stop.name] = [];
    }
    stopsByName[stop.name]!.push(stop);
  }

  const stopsByDistance: Record<string, StopGroup[]> = {};

  for (const [name, nameStops] of Object.entries(stopsByName)) {
    const grouped: StopGroup[] = [];

    for (const stop of nameStops) {
      if (activeStopIds !== null && activeStopIds.size > 0 && !activeStopIds.has(stop.id)) {
        continue;
      }

      const closeEnough = grouped.find((g) => mergedBboxArea(g, stop) < 1e-7);

      if (closeEnough) {
        extendGroup(closeEnough, stop);
        continue;
      }

      grouped.push({
        stops: [stop],
        minLat: stop.lat,
        maxLat: stop.lat,
        minLng: stop.lng,
        maxLng: stop.lng,
      });
    }

    stopsByDistance[name] = grouped;
  }

  return Object.values(stopsByDistance)
    .flatMap((x) => x.map((y) => y.stops))
    .map((a) => {
      const name = a[0]!.name;
      const avgLat = a.reduce((acc, stop) => acc + stop.lat, 0) / a.length;
      const avgLng = a.reduce((acc, stop) => acc + stop.lng, 0) / a.length;
      const ids = a.map((stop) => stop.id);
      return { name, lat: avgLat, lng: avgLng, ids } satisfies GroupedStop;
    });
}

function computeBounds(stops: StopData[]): [[number, number], [number, number]] {
  let minLat = 89.5;
  let maxLat = -89.5;
  let minLng = 89.5;
  let maxLng = -89.5;
  for (const stop of stops) {
    minLat = Math.min(minLat, stop.lat);
    maxLat = Math.max(maxLat, stop.lat);
    minLng = Math.min(minLng, stop.lng);
    maxLng = Math.max(maxLng, stop.lng);
  }
  return [
    [minLng, minLat],
    [maxLng, maxLat],
  ];
}

function extractStopsFromMessage(data: V1Message["d"]): StopData[] | null {
  if (typeof data === "object" && "simpleStops" in data) {
    return (data.simpleStops as (string | number)[][]).map((s) => ({
      id: String(s[0]),
      name: String(s[1]),
      lat: Number(s[2]),
      lng: Number(s[3]),
    }));
  }
  return null;
}

function extractActiveStopIdsFromMessage(data: V1Message["d"]): string[] | null {
  if (typeof data === "object" && "activeStops" in data) {
    return data.activeStops;
  }
  return null;
}

function handleProcessedStops(stops: StopData[]): StopsUpdateResponse {
  cachedStops = stops;
  const bounds = computeBounds(stops);
  const active = cachedActiveStopIds.size > 0 ? cachedActiveStopIds : null;
  cachedGroupedAll = computeGroupedStops(stops, null);
  return {
    type: "stops-update",
    stops,
    bounds,
    grouped: computeGroupedStops(stops, active),
    groupedAll: cachedGroupedAll,
    activeStopIds: [...cachedActiveStopIds],
  };
}

function handleActiveStopIds(activeStopIds: string[]): StopsUpdateResponse {
  cachedActiveStopIds = new Set(activeStopIds);
  const active = cachedActiveStopIds.size > 0 ? cachedActiveStopIds : null;
  return {
    type: "stops-update",
    grouped: computeGroupedStops(cachedStops, active),
    activeStopIds,
  };
}

addEventListener("message", (e: MessageEvent) => {
  const message = e.data as { type: string };

  switch (message.type) {
    case "process-message":
      void handleProcessMessage((message as { type: "process-message"; data: Blob }).data);
      return;
    case "start-fetching-stops":
      startFetchingStops();
      return;
    case "stop-fetching-stops":
      stopFetchingStops();
      return;
    default:
      console.error("[WORKER]", "Unknown message type", message.type);
      return;
  }
});

async function handleProcessMessage(eventData: Blob) {
  const gotEvent = performance.now();
  const buffer = new Uint8Array(await new Response(eventData).arrayBuffer());
  const data = decodeCbor(buffer);
  const endDecode = performance.now();

  const validated = v1MessageSchema.safeParse(data);
  const endValidate = performance.now();

  if (DEBUG) {
    console.log("[WORKER]", "Data parse timings", {
      decode: endDecode - gotEvent,
      validation: endValidate - endDecode,
      total: endValidate - gotEvent,
    });
  }

  if (!validated.success) {
    console.error(validated.error);
    return;
  }

  postMessage({
    type: "processed-message",
    data: validated.data,
  });

  const stops = extractStopsFromMessage(validated.data.d);
  if (stops) {
    postMessage(handleProcessedStops(stops));
    return;
  }

  const activeStopIds = extractActiveStopIdsFromMessage(validated.data.d);
  if (activeStopIds) {
    postMessage(handleActiveStopIds(activeStopIds));
    return;
  }
}

async function fetchAndProcessStops(): Promise<boolean> {
  try {
    const response = await fetch(`${API_URL}/v1/schedule/simple-stops`, {
      headers: {
        accept: "application/cbor,application/json",
      },
    });

    if (!response.ok) {
      console.error("[WORKER]", "Stops fetch failed with status", response.status);
      return false;
    }

    const buffer = new Uint8Array(await response.arrayBuffer());
    const data = decodeCbor(buffer);
    const validated = v1MessageSchema.safeParse(data);

    if (!validated.success) {
      console.error("[WORKER]", "Stops validation error", validated.error);
      return false;
    }

    const stops = extractStopsFromMessage(validated.data.d);
    if (!stops) {
      console.error("[WORKER]", "Stops data not found in response");
      return false;
    }

    postMessage(handleProcessedStops(stops));
    return true;
  } catch (err) {
    console.error("[WORKER]", "Stops fetch error", err);
    return false;
  }
}

async function fetchWithRetry() {
  let success = false;
  while (!success) {
    success = await fetchAndProcessStops();
    if (!success) {
      await new Promise((resolve) => setTimeout(resolve, 5000 + 5000 * Math.random()));
    }
  }
}

function startFetchingStops() {
  stopFetchingStops();
  fetchIntervalId = setInterval(() => void fetchWithRetry(), 60_000 + 60_000 * Math.random());
}

function stopFetchingStops() {
  if (fetchIntervalId !== null) {
    clearInterval(fetchIntervalId);
    fetchIntervalId = null;
  }
}
