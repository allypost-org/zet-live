const APP_CACHE = "zet-app-v1";
const MAP_CACHE = "zet-map-v1";
const MAP_HOST_ORIGINS = [
  "https://tiles.openfreemap.org",
  "https://cdn.allypost.net",
  "https://cdn.igr.ec",
];
const MAP_CACHE_MAX_ENTRIES = 600;

self.addEventListener("install", () => {
  self.skipWaiting();
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
  if (req.method !== "GET") return;

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

  if (MAP_HOST_ORIGINS.includes(url.origin)) {
    event.respondWith(mapCacheFirst(req));
  }
});

async function networkFirstAppShell(req) {
  const cache = await caches.open(APP_CACHE);
  try {
    const res = await fetch(req, { cache: "no-cache" });
    if (res && res.ok) {
      await cache.put(req, res.clone());
    }
    return res;
  } catch (_err) {
    return (await cache.match(req)) || (await cache.match("/")) || Response.error();
  }
}

async function cacheFirst(cacheName, req) {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(req);
  if (cached) return cached;
  try {
    const res = await fetch(req);
    if (res && (res.ok || res.type === "opaque")) {
      await cache.put(req, res.clone());
    }
    return res;
  } catch (_err) {
    return cached || Response.error();
  }
}

async function mapCacheFirst(req) {
  const cache = await caches.open(MAP_CACHE);
  const cached = await cache.match(req);
  if (cached) return cached;
  try {
    const res = await fetch(req);
    if (res && (res.ok || res.type === "opaque")) {
      await cache.put(req, res.clone());
      await trimCache(cache, MAP_CACHE_MAX_ENTRIES);
    }
    return res;
  } catch (_err) {
    return cached || Response.error();
  }
}

async function trimCache(cache, max) {
  const keys = await cache.keys();
  if (keys.length <= max) return;
  for (let i = 0; i < keys.length - max; i += 1) {
    await cache.delete(keys[i]);
  }
}
