import MapGL, {
  NavigationControl,
  GeolocateControl,
  Source,
  Layer,
  type MapRef,
  type MapLayerMouseEvent,
} from "react-map-gl/maplibre";
import { useRef, useCallback, useEffect, useMemo, useState } from "react";
import type { StyleSpecification } from "maplibre-gl";
import type { FeatureCollection } from "geojson";
import mapStyle3d from "@/data/maps/style/3d.json";
import mapStyle3dDark from "@/data/maps/style/3d.dark.json";
import mapStyleFlat from "@/data/maps/style/flat.json";
import mapStyleSatellite from "@/data/maps/style/satellite.json";
import { useStore, type VehicleLocationPair } from "@/store";
import { type MapStyleId, useSetting } from "@/settings";
import { useGeolocationPermission } from "@/hooks/use-geolocation-permission";
import { themeStore } from "@/hooks/use-theme";
import "maplibre-gl/dist/maplibre-gl.css";
import { calculateLatOffset } from "@/utils/map";
import {
  ensureVehicleIcons,
  quantizeBearing,
  vehicleIconName,
  type VehicleIconDescriptor,
} from "@/utils/vehicle-icons";
import {
  ensureStationIcons,
  stationIconName,
  type StationIconDescriptor,
} from "@/utils/gbfs-icons";
import type { VehicleV1 } from "@/app/entity/v1/vehicle";
import type { GbfsStationV1 } from "@/app/entity/v1/gbfs-station";
import {
  appRequestAnimationFrame,
  cancelAnimationOrIdleCallback,
} from "@/utils/polyfill/requestSomeCallback";

const styleMap = new Map<MapStyleId, StyleSpecification>([
  ["3d", mapStyle3d as StyleSpecification],
  ["3d.dark", mapStyle3dDark as StyleSpecification],
  ["flat", mapStyleFlat as StyleSpecification],
  ["satellite", mapStyleSatellite as StyleSpecification],
]);

const emptyGeoJSON: FeatureCollection = {
  type: "FeatureCollection",
  features: [],
};

function createArrowHeadImage(color: string): Promise<HTMLImageElement> {
  const svg = `
    <svg width="50" height="50" viewBox="0 0 50 50" xmlns="http://www.w3.org/2000/svg">
      <path d="M 10,10 L 40,25 L 10,40 Z" fill="${color}" stroke="${color}" stroke-width="2" stroke-linejoin="round"/>
    </svg>
  `;
  const blob = new Blob([svg], { type: "image/svg+xml" });
  const img = new Image(50, 50);
  img.src = URL.createObjectURL(blob);

  return new Promise((resolve) => {
    img.addEventListener("load", () => {
      resolve(img);
    });
  });
}

function buildVehiclesGeoJson(
  vehicles: Map<string, VehicleV1>,
  selectedVehicleId: string | null,
  selectedStopTripIds: Set<string> | null,
  searchMatchedVehicleIds: Set<string> | null,
) {
  const all = Array.from(vehicles.values());
  const filtered = searchMatchedVehicleIds
    ? all.filter((v) => searchMatchedVehicleIds.has(v.getMapId()))
    : all;
  return {
    type: "FeatureCollection" as const,
    features: filtered.map((v) => {
      const isFollowing =
        selectedVehicleId === v.id || (selectedStopTripIds?.has(v.tripId) ?? false);
      const hasFollowing = selectedVehicleId !== null || selectedStopTripIds !== null;
      const isBus = v.routeId.length > 2;

      return {
        type: "Feature" as const,
        geometry: { type: "Point" as const, coordinates: [v.lng, v.lat] as [number, number] },
        properties: {
          id: v.id,
          tripId: v.tripId,
          vehicleType: isBus ? "bus" : "tram",
          themeColor: isBus ? "blue" : "red",
          iconName: vehicleIconName(v.routeId, isBus ? "blue" : "red", quantizeBearing(v.bearing)),
          followingState: isFollowing ? 1 : hasFollowing ? 2 : 0,
          sortKey: isFollowing ? 2 : hasFollowing ? 0 : 1,
        },
      };
    }),
  };
}

function buildDeltaMoveFeatures(deltaMoveLines: VehicleLocationPair[]) {
  return {
    type: "FeatureCollection" as const,
    features: deltaMoveLines.map((x) => ({
      type: "Feature" as const,
      properties: { color: x.color, vehicleType: x.vehicleType },
      beforeId: "route-stop-dot",
      geometry: {
        type: "LineString" as const,
        coordinates: [x.from, x.to],
      },
    })),
  };
}

function buildFollowingRouteFeatures(followingRoute: [number, number][] | null) {
  return followingRoute
    ? {
        type: "FeatureCollection" as const,
        features: [
          {
            type: "Feature" as const,
            properties: { color: "#f0f" },
            geometry: {
              type: "LineString" as const,
              coordinates: followingRoute,
            },
          },
        ],
      }
    : emptyGeoJSON;
}

function buildGbfsStationsGeoJson(
  stations: Map<string, GbfsStationV1>,
  selectedGbfsStationId: string | null,
) {
  const all = Array.from(stations.values());
  return {
    type: "FeatureCollection" as const,
    features: all.map((s) => {
      const bikes = s.numBikesAvailable ?? 0;
      const isSelected = selectedGbfsStationId === s.id;
      return {
        type: "Feature" as const,
        geometry: { type: "Point" as const, coordinates: [s.lng, s.lat] as [number, number] },
        properties: {
          id: s.id,
          iconName: stationIconName(bikes, s.isRenting),
          sortKey: isSelected ? 2 : bikes > 0 ? 1 : 0,
        },
      };
    }),
  };
}

function imperativeSetData(mapRef: { current: MapRef | null }, sourceId: string, data: unknown) {
  const map = mapRef.current?.getMap();
  const source = map?.getSource(sourceId);
  if (source && "setData" in source) {
    (source as { setData: (data: unknown) => void }).setData(data);
  }
}

function useRafSetData(
  mapRef: { current: MapRef | null },
  sourceId: string,
  data: unknown,
  ready = true,
) {
  const rafId = useRef<number | null>(null);
  useEffect(() => {
    if (!ready) return;
    if (rafId.current !== null) cancelAnimationOrIdleCallback(rafId.current);
    const captured = data;
    rafId.current = appRequestAnimationFrame(() => {
      rafId.current = null;
      if (!mapRef.current) return;
      imperativeSetData(mapRef, sourceId, captured);
    });
    return () => {
      if (rafId.current !== null) cancelAnimationOrIdleCallback(rafId.current);
    };
  }, [sourceId, data, mapRef, ready]);
}

export function MapContainer() {
  const mapRef = useRef<MapRef>(null);
  const [iconsReady, setIconsReady] = useState(false);
  const resolvedMapStyleId = themeStore((s) => s.resolvedMapStyleId);
  const mapStyle = styleMap.get(resolvedMapStyleId) ?? (mapStyle3d as StyleSpecification);
  const vehicles = useStore((s) => s.vehicles);
  const selection = useStore((s) => s.selection);
  const vehicleSelection = useStore((s) => s.vehicleSelection);
  const stopSelection = useStore((s) => s.stopSelection);
  const deltaMoveLines = useStore((s) => s.deltaMoveLines);
  const displayedStops = useStore((s) => s.displayedStops);
  const maxBounds = useStore((s) => s.maxBounds);
  const geolocPermission = useGeolocationPermission();
  const searchMatchedVehicleIds = useStore((s) => s.searchMatchedVehicleMapIds);
  const searchMatchedStopIds = useStore((s) => s.searchMatchedStopIds);
  const searchActive = searchMatchedVehicleIds !== null || searchMatchedStopIds !== null;

  const gbfsStations = useStore((s) => s.gbfsStations);
  const showGbfsStations = useSetting("showGbfsStations");
  const showBuses = useSetting("showBuses");
  const showTrams = useSetting("showTrams");

  const flyToTarget = useStore((s) => s.flyToTarget);

  const selectedVehicleId = selection?.type === "vehicle" ? selection.id : null;
  const selectedStopTripIds = stopSelection?.tripIds ?? null;
  const selectedGbfsStationId = selection?.type === "gbfs-station" ? selection.id : null;
  const gbfsLayerVisible =
    showGbfsStations && (selection === null || selection.type === "gbfs-station");
  const followingRoute = vehicleSelection?.route ?? null;

  const selectedVehicle =
    selectedVehicleId !== null ? (vehicles.get(`vehicle-${selectedVehicleId}`) ?? null) : null;
  const nextStopId = selectedVehicle?.nextStopId ?? null;

  const handleClick = useCallback((e: MapLayerMouseEvent) => {
    const vehicleFeature = e.features?.find((x) => x.source === "vehicles");
    if (vehicleFeature) {
      const props = vehicleFeature.properties as Record<string, unknown>;
      useStore.getState().selectVehicle(String(props.id), String(props.tripId), true);
      return;
    }

    const stationFeature = e.features?.find((x) => x.source === "gbfs-stations");
    if (stationFeature) {
      const props = stationFeature.properties as Record<string, unknown>;
      useStore.getState().selectGbfsStation(String(props.id), true);
      return;
    }

    const stopFeature = e.features?.find((x) => x.source === "route-stops");
    if (stopFeature) {
      const stopIds = JSON.parse(
        ((stopFeature.properties as Record<string, unknown>)?.ids as string) ?? "[]",
      ) as string[];
      useStore.getState().selectStop(stopIds);
      return;
    }

    useStore.getState().clearSelection();
  }, []);

  const onLoad = useCallback(() => {
    const map = mapRef.current?.getMap();
    if (!map) return;

    void (async () => {
      const arrowImage = await createArrowHeadImage("#00000077");
      if (!map.hasImage("arrow-head")) {
        map.addImage("arrow-head", arrowImage);
      }

      map.setMaxBounds(useStore.getState().maxBounds);
      setIconsReady(true);
      useStore.setState({ mapReady: true });
    })();
  }, []);

  const onDragStart = useCallback(() => {
    useStore.getState().setFollowEnabled(false);
  }, []);

  useEffect(() => {
    if (selection?.type !== "vehicle") return;
    if (!vehicleSelection?.followEnabled) return;
    const vehicle = vehicles.get(`vehicle-${selection.id}`);
    if (!vehicle) return;

    const map = mapRef.current?.getMap();
    if (!map) return;

    const offset = calculateLatOffset(mapRef.current?.getMap());

    map.easeTo({
      center: [vehicle.lng, vehicle.lat - offset],
      duration: 500,
    });
  }, [selection, vehicleSelection, vehicles]);

  useEffect(() => {
    const map = mapRef.current?.getMap();
    if (!map) {
      return;
    }

    map.on("mouseenter", "route-stops-label", () => {
      map.getCanvas().style.cursor = "pointer";
    });
    map.on("mouseleave", "route-stops-label", () => {
      map.getCanvas().style.cursor = "";
    });

    map.on("mouseenter", "vehicle-markers", () => {
      map.getCanvas().style.cursor = "pointer";
    });
    map.on("mouseleave", "vehicle-markers", () => {
      map.getCanvas().style.cursor = "";
    });

    map.on("mouseenter", "gbfs-station-markers", () => {
      map.getCanvas().style.cursor = "pointer";
    });
    map.on("mouseleave", "gbfs-station-markers", () => {
      map.getCanvas().style.cursor = "";
    });
  }, []);

  useEffect(() => {
    const map = mapRef.current?.getMap();
    if (!map) return;
    map.setMaxBounds(maxBounds);
  }, [maxBounds]);

  useEffect(() => {
    if (!flyToTarget) return;
    const offset = calculateLatOffset(mapRef.current?.getMap());
    mapRef.current?.flyTo({
      center: [flyToTarget.longitude, flyToTarget.latitude - offset],
      duration: 1000,
    });
    useStore.setState({ flyToTarget: null });
  }, [flyToTarget]);

  const vehicleIconsToEnsure = useMemo<VehicleIconDescriptor[]>(() => {
    const unique = new Map<string, VehicleIconDescriptor>();
    for (const v of vehicles.values()) {
      const color = v.routeId.length > 2 ? "blue" : "red";
      const qBearing = quantizeBearing(v.bearing);
      const key = `${v.routeId}|${color}|${qBearing ?? "none"}`;
      if (!unique.has(key)) {
        unique.set(key, {
          routeId: v.routeId,
          color,
          qBearing,
        });
      }
    }
    return [...unique.values()];
  }, [vehicles]);

  useEffect(() => {
    if (!iconsReady) return;
    const map = mapRef.current?.getMap();
    if (!map) return;
    ensureVehicleIcons(map, vehicleIconsToEnsure);
  }, [iconsReady, vehicleIconsToEnsure]);

  const vehiclesRafId = useRef<number | null>(null);
  const vehiclesGeoJson = useMemo(
    () =>
      buildVehiclesGeoJson(
        vehicles,
        selectedVehicleId,
        selectedStopTripIds,
        searchMatchedVehicleIds,
      ),
    [vehicles, selectedVehicleId, selectedStopTripIds, searchMatchedVehicleIds],
  );
  useEffect(() => {
    if (!iconsReady) return;
    cancelAnimationOrIdleCallback(vehiclesRafId.current);
    const data = vehiclesGeoJson;
    vehiclesRafId.current = appRequestAnimationFrame(() => {
      vehiclesRafId.current = null;
      const map = mapRef.current?.getMap();
      if (!map) return;
      ensureVehicleIcons(
        map,
        data.features
          .map((f) => {
            const props = f.properties as { iconName: string };
            const match = /^vehicle-(red|blue)-r([^-]+)-b(\d+|none)$/.exec(props.iconName);
            if (!match) return null;
            return {
              routeId: decodeURIComponent(match[2]!),
              color: match[1],
              qBearing: match[3] === "none" ? null : Number(match[3]),
            };
          })
          .filter((d): d is VehicleIconDescriptor => d !== null),
      );
      imperativeSetData(mapRef, "vehicles", data);
    });
    return () => {
      cancelAnimationOrIdleCallback(vehiclesRafId.current);
    };
  }, [iconsReady, vehiclesGeoJson]);

  const deltaMoveFeatures = useMemo(() => buildDeltaMoveFeatures(deltaMoveLines), [deltaMoveLines]);
  useRafSetData(mapRef, "delta-move-lines", deltaMoveFeatures);

  const routeStopsFeatures = useMemo(() => {
    const filtered = searchMatchedStopIds
      ? displayedStops.filter((s) => s.ids.some((id) => searchMatchedStopIds.has(id)))
      : displayedStops;
    return {
      type: "FeatureCollection" as const,
      features: filtered.map((stop) => ({
        type: "Feature" as const,
        properties: {
          name: stop.name,
          ids: JSON.stringify(stop.ids),
          isNext: nextStopId !== null && stop.ids.includes(nextStopId),
        },
        geometry: {
          type: "Point" as const,
          coordinates: [stop.lng, stop.lat],
        },
      })),
    };
  }, [displayedStops, nextStopId, searchMatchedStopIds]);
  useRafSetData(mapRef, "route-stops", routeStopsFeatures);

  const followingRouteFeatures = useMemo(
    () => buildFollowingRouteFeatures(followingRoute),
    [followingRoute],
  );
  useRafSetData(mapRef, "current-following-route", followingRouteFeatures);

  const gbfsStationsFeatures = useMemo(
    () => buildGbfsStationsGeoJson(gbfsStations, selectedGbfsStationId),
    [gbfsStations, selectedGbfsStationId],
  );
  useRafSetData(mapRef, "gbfs-stations", gbfsStationsFeatures, iconsReady);

  const stationIconsToEnsure = useMemo<StationIconDescriptor[]>(() => {
    const unique = new Map<string, StationIconDescriptor>();
    for (const s of gbfsStations.values()) {
      const count = s.numBikesAvailable ?? 0;
      const key = `${count}|${s.isRenting}`;
      if (!unique.has(key)) {
        unique.set(key, { count, isRenting: s.isRenting });
      }
    }
    return [...unique.values()];
  }, [gbfsStations]);

  useEffect(() => {
    if (!iconsReady) return;
    const map = mapRef.current?.getMap();
    if (!map) return;
    ensureStationIcons(map, stationIconsToEnsure);
  }, [iconsReady, stationIconsToEnsure]);

  const isFollowingSomething = selection !== null;

  useEffect(() => {
    const map = mapRef.current?.getMap();
    if (!map) return;
    map.setLayoutProperty("route-stops-label", "text-allow-overlap", isFollowingSomething);
    map.setLayoutProperty("route-stops-label", "text-ignore-placement", isFollowingSomething);
  }, [isFollowingSomething]);

  const vehicleMarkerLayout: Record<string, unknown> = useMemo(
    () => ({
      "icon-image": ["get", "iconName"],
      "symbol-z-order": "source",
      "icon-allow-overlap": true,
      "icon-ignore-placement": true,
      "symbol-sort-key": ["get", "sortKey"],
    }),
    [],
  );

  const vehicleMarkerPaint: Record<string, unknown> | undefined = useMemo(
    () =>
      searchActive
        ? undefined
        : {
            "icon-opacity": ["case", ["==", ["get", "followingState"], 2], 0.1, 1],
          },
    [searchActive],
  );

  return (
    <div className="relative h-full w-full">
      <MapGL
        key={resolvedMapStyleId}
        ref={mapRef}
        mapStyle={mapStyle}
        initialViewState={{
          longitude: 16,
          latitude: 45.8,
          zoom: 12,
        }}
        hash
        // @ts-expect-error antialias exists in maplibre-gl but not in mapbox-gl types
        antialias
        interactiveLayerIds={["route-stops-label", "vehicle-markers", "gbfs-station-markers"]}
        onClick={handleClick}
        onLoad={onLoad}
        onDragStart={onDragStart}
        className="h-full w-full"
      >
        {/* @ts-expect-error visualizeZoom exists in maplibre-gl but not in mapbox-gl types */}
        <NavigationControl visualizeZoom visualizePitch />

        <GeolocateControl
          key={
            geolocPermission === "granted"
              ? "geo-granted"
              : geolocPermission === "denied"
                ? "geo-denied"
                : "geo-other"
          }
          positionOptions={{ enableHighAccuracy: true }}
          trackUserLocation
          showAccuracyCircle
          showUserLocation
        />

        {iconsReady && (
          <Source id="vehicles" type="geojson" data={emptyGeoJSON}>
            <Layer
              id="vehicle-markers"
              type="symbol"
              layout={vehicleMarkerLayout}
              paint={vehicleMarkerPaint}
              filter={["match", ["get", "vehicleType"], "tram", showTrams, "bus", showBuses, true]}
            />
          </Source>
        )}

        {iconsReady && (
          <Source id="gbfs-stations" type="geojson" data={emptyGeoJSON}>
            <Layer
              id="gbfs-station-markers"
              type="symbol"
              beforeId="route-stop-dot"
              layout={{
                "icon-image": ["get", "iconName"],
                "icon-allow-overlap": true,
                "icon-ignore-placement": true,
                "symbol-z-order": "source",
                "symbol-sort-key": ["get", "sortKey"],
                visibility: gbfsLayerVisible ? "visible" : "none",
              }}
            />
          </Source>
        )}

        <Source id="delta-move-lines" type="geojson" data={emptyGeoJSON}>
          <Layer
            id="delta-move-lines"
            type="line"
            filter={["match", ["get", "vehicleType"], "tram", showTrams, "bus", showBuses, true]}
            paint={{
              "line-width": 5,
              "line-color": ["get", "color"],
              "line-opacity": 0.5,
            }}
            layout={{
              "line-join": "round",
              "line-cap": "round",
              visibility: searchActive ? "none" : "visible",
            }}
          />
          <Layer
            id="delta-move-lines-arrow"
            type="symbol"
            filter={["match", ["get", "vehicleType"], "tram", showTrams, "bus", showBuses, true]}
            layout={{
              "symbol-placement": "line",
              "symbol-avoid-edges": false,
              "symbol-spacing": 1,
              "icon-image": "arrow-head",
              "text-ignore-placement": true,
              "icon-size": 0.25,
              visibility: searchActive ? "none" : "visible",
            }}
          />
        </Source>

        <Source id="current-following-route" type="geojson" data={emptyGeoJSON}>
          <Layer
            id="current-following-route"
            type="line"
            paint={{
              "line-width": 5,
              "line-color": ["get", "color"],
              "line-opacity": 0.5,
            }}
            layout={{
              "line-join": "round",
              "line-cap": "round",
            }}
          />
          <Layer
            id="current-following-route-arrow"
            type="symbol"
            layout={{
              "symbol-placement": "line",
              "symbol-spacing": 1,
              "icon-allow-overlap": true,
              "icon-image": "arrow-head",
              "icon-size": 0.25,
              visibility: "visible",
            }}
          />
        </Source>
      </MapGL>
    </div>
  );
}
