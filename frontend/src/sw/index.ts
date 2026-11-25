import { MAP_CACHE_ORIGINS } from "@/data/maps/cache-origins";

declare const self: ServiceWorkerGlobalScope;

const APP_CACHE = "zet-app-v1";
const MAP_CACHE = "zet-map-v1";
const MAP_CACHE_MAX_ENTRIES = 600;
// Cross-origin responses do not expose `Date` or `Age` to scripts, so the
// worker records its own fetch time on each cached map response.
const FETCHED_AT_HEADER = "x-sw-fetched-at";
// Upper limit for heuristic freshness when a response has no explicit lifetime.
const MAX_HEURISTIC_FRESHNESS_MS = 24 * 60 * 60 * 1000;

self.addEventListener("install", () => {
  void self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys.filter((k) => k !== APP_CACHE && k !== MAP_CACHE).map((k) => caches.delete(k)),
      );
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") {
    return;
  }

  const url = new URL(req.url);

  if (req.mode === "navigate") {
    event.respondWith(networkFirstAppShell(req));
    return;
  }

  if (url.origin === self.location.origin) {
    if (url.pathname.startsWith("/_static/")) {
      event.respondWith(cacheFirst(APP_CACHE, req));
    }
    return;
  }

  if (MAP_CACHE_ORIGINS.includes(url.origin)) {
    event.respondWith(mapCache(event));
  }
});

async function networkFirstAppShell(req: Request): Promise<Response> {
  const cache = await caches.open(APP_CACHE);
  try {
    const res = await fetch(req, { cache: "no-cache" });
    if (res.ok) {
      await cache.put(req, res.clone());
    }
    return res;
  } catch {
    return (await cache.match(req)) ?? (await cache.match("/")) ?? Response.error();
  }
}

async function cacheFirst(cacheName: string, req: Request): Promise<Response> {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(req);
  if (cached) {
    return cached;
  }
  try {
    const res = await fetch(req);
    if (res.ok || res.type === "opaque") {
      await cache.put(req, res.clone());
    }
    return res;
  } catch {
    return Response.error();
  }
}

/**
 * Serves map resources from the cache while they are fresh, by the response
 * `Cache-Control` and `Expires` headers. A stale entry is fetched again; the
 * stale copy is used only when the network fails.
 */
async function mapCache(event: FetchEvent): Promise<Response> {
  const req = event.request;
  const cache = await caches.open(MAP_CACHE);
  const cached = await cache.match(req);
  if (cached && isFresh(cached, Date.now())) {
    return cached;
  }

  let res: Response;
  try {
    res = await fetch(req);
  } catch {
    return cached ?? Response.error();
  }

  if (res.ok && isStorable(res)) {
    event.waitUntil(storeMapResponse(cache, req, res.clone()));
  } else if (cached && res.status >= 500) {
    return cached;
  }
  return res;
}

async function storeMapResponse(cache: Cache, req: Request, res: Response): Promise<void> {
  const headers = new Headers(res.headers);
  headers.set(FETCHED_AT_HEADER, String(Date.now()));
  const body = await res.blob();
  await cache.put(
    req,
    new Response(body, { status: res.status, statusText: res.statusText, headers }),
  );
  await trimCache(cache, MAP_CACHE_MAX_ENTRIES);
}

function parseCacheControl(headers: Headers): Map<string, string | undefined> {
  const directives = new Map<string, string | undefined>();
  for (const part of (headers.get("cache-control") ?? "").split(",")) {
    const [name, value] = part.trim().split("=", 2);
    if (name) {
      directives.set(name.toLowerCase(), value?.replace(/^"|"$/g, ""));
    }
  }
  return directives;
}

function isStorable(res: Response): boolean {
  return !parseCacheControl(res.headers).has("no-store");
}

/** Freshness lifetime in milliseconds, after RFC 9111 section 4.2.1. */
function freshnessLifetime(headers: Headers, fetchedAt: number): number {
  const directives = parseCacheControl(headers);
  if (directives.has("no-cache")) {
    return 0;
  }

  const maxAge = Number.parseInt(directives.get("max-age") ?? "", 10);
  if (Number.isFinite(maxAge)) {
    return Math.max(0, maxAge) * 1000;
  }

  const expires = headers.get("expires");
  if (expires) {
    const time = Date.parse(expires);
    // An invalid date, such as "0", means that the response is already stale.
    return Number.isNaN(time) ? 0 : Math.max(0, time - fetchedAt);
  }

  // Heuristic freshness: 10% of the time since the last modification.
  const lastModified = Date.parse(headers.get("last-modified") ?? "");
  if (Number.isNaN(lastModified)) {
    return 0;
  }
  return Math.min(Math.max(0, (fetchedAt - lastModified) / 10), MAX_HEURISTIC_FRESHNESS_MS);
}

function isFresh(res: Response, now: number): boolean {
  const fetchedAt = Number(res.headers.get(FETCHED_AT_HEADER));
  // Entries from older worker versions have no fetch time. Treat them as stale.
  if (!fetchedAt) {
    return false;
  }
  return now - fetchedAt < freshnessLifetime(res.headers, fetchedAt);
}

async function trimCache(cache: Cache, max: number): Promise<void> {
  const keys = await cache.keys();
  for (const key of keys.slice(0, Math.max(0, keys.length - max))) {
    await cache.delete(key);
  }
}
