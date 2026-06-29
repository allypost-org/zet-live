import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import Fuse from "fuse.js";
import { useStore, type GroupedStop } from "@/store";
import type { VehicleV1 } from "@/app/entity/v1/vehicle";

type VehicleRouteGroup = {
  routeId: string;
  displayName: string;
  vehicles: VehicleV1[];
};

type UnifiedSearchResult =
  | { type: "station"; score: number; item: GroupedStop }
  | { type: "vehicle"; score: number; item: VehicleRouteGroup };

type CategoryFilters = {
  stations: boolean;
  trams: boolean;
  buses: boolean;
};

type SearchableItem =
  | { type: "station"; label: string; routeId: string; data: GroupedStop }
  | { type: "tram"; label: string; routeId: string; data: VehicleRouteGroup }
  | { type: "bus"; label: string; routeId: string; data: VehicleRouteGroup };

function isTramRoute(routeId: string): boolean {
  return routeId.length <= 2;
}

export function SearchBar() {
  const [query, setQuery] = useState("");
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const [filters, setFilters] = useState<CategoryFilters>({
    stations: true,
    trams: true,
    buses: true,
  });
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [isSearchActive, setIsSearchActive] = useState(false);
  const vehicles = useStore((s) => s.vehicles);
  const stopsGrouped = useStore((s) => s.stopsGrouped);

  const searchableItems = useMemo<SearchableItem[]>(() => {
    if (!isSearchActive) return [];

    const items: SearchableItem[] = [];

    for (const stop of stopsGrouped) {
      items.push({
        type: "station",
        label: stop.name,
        routeId: "",
        data: stop,
      });
    }

    const groups = new Map<string, VehicleRouteGroup>();
    for (const v of vehicles.values()) {
      const displayName = v.getDisplayName();
      const key = `${v.routeId}|${displayName}`;
      const existing = groups.get(key);
      if (existing) {
        existing.vehicles.push(v);
      } else {
        groups.set(key, {
          routeId: v.routeId,
          displayName,
          vehicles: [v],
        });
      }
    }

    for (const group of groups.values()) {
      items.push({
        type: isTramRoute(group.routeId) ? "tram" : "bus",
        label: group.displayName,
        routeId: group.routeId,
        data: group,
      });
    }

    return items;
  }, [isSearchActive, vehicles, stopsGrouped]);

  const fuse = useMemo(
    () =>
      new Fuse(searchableItems, {
        keys: [
          { name: "routeId", weight: 1.0 },
          { name: "label", weight: 0.7 },
        ],
        threshold: 0.4,
        ignoreLocation: true,
        includeScore: true,
        shouldSort: true,
      }),
    [searchableItems],
  );

  const results = useMemo<UnifiedSearchResult[]>(() => {
    const normalizedQuery = query.trim().toLowerCase();
    if (!normalizedQuery) return [];

    if (filters.trams || filters.buses) {
      const exactMatches = searchableItems.filter(
        (item) => item.type !== "station" && item.routeId.toLowerCase() === normalizedQuery,
      );
      const filtered = exactMatches.filter((item) => {
        if (item.type === "tram" && !filters.trams) return false;
        if (item.type === "bus" && !filters.buses) return false;
        return true;
      });

      if (filtered.length > 0) {
        return filtered.map((item) => ({
          type: "vehicle" as const,
          score: 0,
          item: item.data as VehicleRouteGroup,
        }));
      }
    }

    const fuseResults = fuse.search(normalizedQuery, { limit: 50 });
    const combined: UnifiedSearchResult[] = [];

    for (const r of fuseResults) {
      const item = r.item;
      if (item.type === "station" && !filters.stations) continue;
      if (item.type === "tram" && !filters.trams) continue;
      if (item.type === "bus" && !filters.buses) continue;

      if (item.type === "station") {
        combined.push({ type: "station", score: r.score ?? 1, item: item.data });
      } else {
        combined.push({ type: "vehicle", score: r.score ?? 1, item: item.data });
      }
    }

    return combined.slice(0, 10);
  }, [query, fuse, filters, searchableItems]);

  useEffect(() => {
    if (!query.trim()) {
      useStore.setState({ searchMatchedVehicleMapIds: null, searchMatchedStopIds: null });
      if (!document.activeElement || document.activeElement !== inputRef.current) {
        setIsSearchActive(false);
      }
      return;
    }

    const vehicleIds = new Set<string>();
    const stopIds = new Set<string>();

    for (const r of results) {
      if (r.type === "station") {
        for (const id of r.item.ids) {
          stopIds.add(id);
        }
      } else {
        for (const v of r.item.vehicles) {
          vehicleIds.add(v.getMapId());
        }
      }
    }

    useStore.setState({
      searchMatchedVehicleMapIds: vehicleIds,
      searchMatchedStopIds: stopIds,
    });
  }, [results, query]);

  useEffect(() => {
    return () => {
      useStore.setState({ searchMatchedVehicleMapIds: null, searchMatchedStopIds: null });
    };
  }, []);

  const handleSelectStation = useCallback((station: GroupedStop) => {
    useStore.setState({ flyToTarget: { longitude: station.lng, latitude: station.lat } });
    useStore.getState().selectStop(station.ids);
    setQuery("");
    inputRef.current?.blur();
  }, []);

  const handleSelectVehicle = useCallback((group: VehicleRouteGroup) => {
    const first = group.vehicles[0];
    if (first) {
      useStore.getState().selectVehicle(first.id, first.tripId, true);
    }
    setQuery("");
    inputRef.current?.blur();
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setQuery("");
        inputRef.current?.blur();
        return;
      }

      if (e.key === "ArrowDown") {
        e.preventDefault();
        setFocusedIndex((i) => Math.min(i + 1, results.length - 1));
        return;
      }

      if (e.key === "ArrowUp") {
        e.preventDefault();
        setFocusedIndex((i) => Math.max(i - 1, 0));
        return;
      }

      if (e.key === "Enter" && focusedIndex >= 0 && focusedIndex < results.length) {
        e.preventDefault();
        const result = results[focusedIndex];
        if (!result) return;
        if (result.type === "station") {
          handleSelectStation(result.item);
        } else {
          handleSelectVehicle(result.item);
        }
      }
    },
    [focusedIndex, results, handleSelectStation, handleSelectVehicle],
  );

  useEffect(() => {
    setFocusedIndex(-1);
  }, [query, filters]);

  const hasQuery = query.trim().length > 0;
  const hasResults = results.length > 0;

  const toggleFilter = useCallback((key: keyof CategoryFilters) => {
    setFilters((prev) => ({ ...prev, [key]: !prev[key] }));
  }, []);

  return (
    <div
      ref={containerRef}
      className="bg-surface-overlay pointer-events-auto w-full max-w-md overflow-hidden rounded-xl shadow-md backdrop-blur-sm"
    >
      <div className="flex items-center gap-2 px-3 py-2">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="text-on-surface-muted shrink-0"
        >
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.3-4.3" />
        </svg>
        <input
          ref={inputRef}
          type="text"
          value={query}
          onInput={(e) => {
            setQuery((e.target as HTMLInputElement).value);
          }}
          onFocus={() => {
            setIsSearchActive(true);
          }}
          onBlur={() => {
            if (!query.trim()) setIsSearchActive(false);
          }}
          onKeyDown={handleKeyDown}
          placeholder="Search stations or routes..."
          className="text-on-surface placeholder:text-on-surface-faint min-w-0 flex-1 bg-transparent text-sm outline-none"
        />
        {hasQuery && (
          <button
            type="button"
            aria-label="Clear search"
            onMouseDown={(e) => {
              e.preventDefault();
            }}
            onClick={() => {
              setQuery("");
            }}
            className="text-on-surface-faint hover:text-on-surface-muted flex shrink-0 cursor-pointer items-center justify-center rounded p-0.5 transition-colors"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M18 6 6 18" />
              <path d="m6 6 12 12" />
            </svg>
          </button>
        )}
      </div>

      {hasQuery && (
        <div className="border-outline-variant border-t">
          <div className="flex items-center gap-1.5 px-3 py-1.5">
            <button
              type="button"
              onClick={() => {
                toggleFilter("stations");
              }}
              className={`rounded-full px-2 py-0.5 text-[10px] font-semibold tracking-wide transition-colors ${
                filters.stations
                  ? "bg-surface-dim text-on-surface-variant"
                  : "bg-surface-dim text-on-surface-faint"
              }`}
            >
              Stations
            </button>
            <button
              type="button"
              onClick={() => {
                toggleFilter("trams");
              }}
              className={`rounded-full px-2 py-0.5 text-[10px] font-semibold tracking-wide transition-colors ${
                filters.trams
                  ? "bg-danger-container text-on-danger-container"
                  : "bg-surface-dim text-on-surface-faint"
              }`}
            >
              Trams
            </button>
            <button
              type="button"
              onClick={() => {
                toggleFilter("buses");
              }}
              className={`rounded-full px-2 py-0.5 text-[10px] font-semibold tracking-wide transition-colors ${
                filters.buses
                  ? "bg-primary-container text-on-primary-container"
                  : "bg-surface-dim text-on-surface-faint"
              }`}
            >
              Buses
            </button>
          </div>

          <div className="border-outline-variant max-h-80 overflow-y-auto border-t" role="listbox">
            {results.map((result, idx) => (
              <button
                key={
                  result.type === "station"
                    ? `s-${result.item.ids.join(",")}`
                    : `v-${result.item.routeId}-${result.item.vehicles[0]?.id ?? idx}`
                }
                type="button"
                role="option"
                aria-selected={focusedIndex === idx}
                className="text-on-surface-variant hover:bg-surface-hover aria-selected:bg-primary-container aria-selected:text-on-primary-container flex w-full cursor-pointer flex-col gap-0.5 border-none bg-transparent px-3 py-1.5 text-left text-sm transition-colors"
                onMouseDown={(e) => {
                  e.preventDefault();
                }}
                onClick={() => {
                  if (result.type === "station") {
                    handleSelectStation(result.item);
                  } else {
                    handleSelectVehicle(result.item);
                  }
                }}
                onFocus={() => {
                  setFocusedIndex(idx);
                }}
              >
                {result.type === "station" ? (
                  <>
                    <span className="flex items-center gap-2">
                      <svg
                        xmlns="http://www.w3.org/2000/svg"
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="2"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        className="text-on-surface-faint shrink-0"
                      >
                        <path d="M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0Z" />
                        <circle cx="12" cy="10" r="3" />
                      </svg>
                      <span className="font-medium">{result.item.name}</span>
                    </span>
                    <span className="text-on-surface-faint pl-6 text-xs">
                      {result.item.ids.length === 1 ? "1 stop" : `${result.item.ids.length} stops`}
                    </span>
                  </>
                ) : (
                  <>
                    <span className="flex items-center gap-2">
                      <span
                        className={`inline-flex items-center justify-center rounded px-1.5 py-0.5 text-xs font-bold ${result.item.routeId.length > 2 ? "bg-primary-container text-on-primary-container" : "bg-danger-container text-on-danger-container"}`}
                      >
                        {result.item.routeId}
                      </span>
                      <span className="font-medium">{result.item.displayName}</span>
                    </span>
                    <span className="text-on-surface-faint pl-7 text-xs">
                      {result.item.vehicles.length === 1
                        ? "1 vehicle"
                        : `${result.item.vehicles.length} vehicles`}
                    </span>
                  </>
                )}
              </button>
            ))}

            {!hasResults && (
              <div className="text-on-surface-faint px-3 py-3 text-center text-sm" role="status">
                No results found
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
