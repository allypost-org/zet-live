/**
 * Origins of map resources that the service worker caches.
 *
 * The styles in `style/` link to some of these origins directly. Others are
 * linked only from fetched documents: the `ofm-maps.allypost.net/planet`
 * TileJSON points to tiles on `tiles.openfreemap.org`. Update this list when a
 * style or its TileJSON moves to a different host.
 */
export const MAP_CACHE_ORIGINS: readonly string[] = [
  "https://tiles.openfreemap.org",
  "https://ofm-maps.allypost.net",
  "https://cdn.allypost.net",
  "https://cdn.igr.ec",
];
