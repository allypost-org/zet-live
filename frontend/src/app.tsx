import { lazy, Suspense, useEffect, useMemo, type ReactNode } from "react";
import { BottomSheet } from "@/components/bottom-sheet";
import { StopSheet } from "@/components/stop-sheet";
import { VehicleSheet } from "@/components/vehicle-sheet";
import { GbfsStationSheet } from "@/components/gbfs-station-sheet";
import { StatusBar } from "@/components/status-bar";
import { SearchBar } from "@/components/search-bar";
import { LoadingScreen } from "@/components/loading-screen";
import { DepartureSummary } from "@/components/departures-board";
import { GroupedStopDepartures } from "@/components/grouped-stop-departures";
import { LineBadge } from "@/components/line-badge";
import {
  CenterIcon,
  FitAllIcon,
  ShareIcon,
  SheetActions,
  shareSheetLink,
  type SheetAction,
} from "@/components/sheet-actions";
import { Toaster } from "sonner";
import { useWebSocket } from "@/hooks/use-websocket";
import { useUrlSync } from "@/hooks/use-url-sync";
import { useSelectionFetcher } from "@/hooks/use-selection-fetcher";
import { useVersionCheck } from "@/hooks/use-version-check";
import { useTheme } from "@/hooks/use-theme";
import { useCapabilities } from "@/hooks/use-capabilities";
import { useAuth } from "@/hooks/use-auth";
import { useSettingsSync } from "@/hooks/use-settings-sync";
import { useStore } from "@/store";
import { findNextStopIndex } from "@/app/trip-stop-times";
import { SettingsButton, SettingsModal } from "./components/settings-modal";
import { FeedbackButton, FeedbackModal } from "./components/feedback-modal";
import { AuthButton } from "./components/auth-button";
import { AuthModal } from "./components/auth-modal";
import { NoticeBar } from "./components/notice-bar";
import { useWakeLock } from "@/hooks/use-wake-lock";
import { useSelectedStopTripIds } from "@/hooks/use-selected-stop-trips";
import { boundsFromPoints } from "@/utils/map";
import { useFeatureFlag } from "@/feature-flags-store";
import { useSetting } from "./settings";
import {
  PLAUSIBLE_API_URL,
  PLAUSIBLE_SCRIPT_URL,
  PLAUSIBLE_SITE_URL,
  SITE_TITLE,
} from "./app/consts";
import imgAppleTouchIcon from "@/assets/img/favicon/apple-touch-icon.png";
import imgFavicon16x16 from "@/assets/img/favicon/favicon-16x16.png";
import imgFavicon32x32 from "@/assets/img/favicon/favicon-32x32.png";
import imgFaviconSvg from "@/assets/img/favicon/favicon.svg";

const MapContainer = lazy(() =>
  import("@/components/map-container").then((m) => ({ default: m.MapContainer })),
);

export function App() {
  useWebSocket();
  useUrlSync();
  useSelectionFetcher();
  useVersionCheck();
  useTheme();
  useCapabilities();
  useAuth();
  useSettingsSync();

  const wakeLockEnabled = useSetting("wakeLockEnabled");
  useWakeLock(wakeLockEnabled);

  const stopBoardFlag = useFeatureFlag("stop_departures");

  const selection = useStore((s) => s.selection);
  const vehicleSelection = useStore((s) => s.vehicleSelection);
  const stopSelection = useStore((s) => s.stopSelection);
  const vehicles = useStore((s) => s.vehicles);
  const displayedStops = useStore((s) => s.displayedStops);
  const gbfsStations = useStore((s) => s.gbfsStations);

  const selectVehicle = useStore((s) => s.selectVehicle);
  const selectStop = useStore((s) => s.selectStop);
  const clearSelection = useStore((s) => s.clearSelection);
  const setFollowEnabled = useStore((s) => s.setFollowEnabled);

  const selectedVehicle =
    selection?.type === "vehicle" ? (vehicles.get(`vehicle-${selection.id}`) ?? null) : null;
  const selectedGbfsStation =
    selection?.type === "gbfs-station"
      ? (gbfsStations.get(`gbfs-station-${selection.id}`) ?? null)
      : null;
  const selectedStop =
    selection?.type === "stop" && stopSelection
      ? { name: stopSelection.name, ids: selection.ids, routes: stopSelection.routes }
      : null;

  const tripStopTimes = vehicleSelection?.tripStopTimes ?? null;
  const stopArrivalTimes = stopSelection?.arrivalTimes ?? null;
  const visibleBoards = stopSelection?.departureBoards.filter(
    (board) => stopSelection.focusedStopId === null || board.stopId === stopSelection.focusedStopId,
  );
  const stopDepartures = visibleBoards
    ?.flatMap((board) => board.departures ?? [])
    .sort((a, b) => {
      const aTime = a.kind === "live" ? a.predictedTime : a.scheduledTime;
      const bTime = b.kind === "live" ? b.predictedTime : b.scheduledTime;
      return aTime.getTime() - bTime.getTime();
    });
  const tripFetchError = vehicleSelection?.fetchError ?? null;
  const followEnabled = vehicleSelection?.followEnabled ?? false;

  const selectionType = selection?.type ?? null;
  const stopName = stopSelection?.name ?? null;
  const documentSubject = useMemo(() => {
    if (selectedVehicle) {
      return `${selectedVehicle.routeId} ${selectedVehicle.getDisplayName()}`;
    }
    if (selectionType === "stop" && stopName) {
      return `${stopName} (stanica)`;
    }
    if (selectedGbfsStation) {
      return `${selectedGbfsStation.getDisplayName()} [Bajs]`;
    }
    return "Trenutno Stanje";
  }, [selectedVehicle, selectedGbfsStation, selectionType, stopName]);
  const documentTitle = `${documentSubject} | ${SITE_TITLE}`;

  const nextStopIndex = selectedVehicle
    ? findNextStopIndex(
        displayedStops,
        selectedVehicle.nextStopSequence,
        selectedVehicle.nextStopId,
      )
    : -1;

  const isOpen = selectedVehicle !== null || selectedStop !== null || selectedGbfsStation !== null;

  const selectedStopTripIds = useSelectedStopTripIds();

  // `displayedStops` holds exactly the selected stop(s) while a stop is selected, so it
  // is also the authoritative position for framing the camera.
  function fitStopBounds(includeVehicles: boolean) {
    const points: [number, number][] = displayedStops.map((stop) => [stop.lng, stop.lat]);
    if (points.length === 0) return;

    if (includeVehicles && selectedStopTripIds) {
      for (const vehicle of vehicles.values()) {
        if (selectedStopTripIds.has(vehicle.tripId)) {
          points.push([vehicle.lng, vehicle.lat]);
        }
      }
    }

    const bounds = boundsFromPoints(points);
    if (bounds) useStore.setState({ fitBoundsTarget: bounds });
  }

  const stopActions: SheetAction[] = selectedStop
    ? [
        {
          key: "share",
          label: "Share",
          icon: <ShareIcon />,
          onClick: () => {
            const params = new URLSearchParams();
            for (const id of selectedStop.ids) params.append("stop", id);
            shareSheetLink(selectedStop.name, params);
          },
        },
        {
          key: "center",
          label: "Center",
          icon: <CenterIcon />,
          onClick: () => {
            fitStopBounds(false);
          },
        },
        {
          key: "fit-all",
          label: "Fit all",
          icon: <FitAllIcon />,
          onClick: () => {
            fitStopBounds(true);
          },
          // Trips are still loading; framing now would silently ignore the vehicles.
          disabled: selectedStopTripIds === null,
        },
      ]
    : [];

  let sheetTitle: ReactNode = null;
  let minimizedBody: ReactNode | undefined;

  if (selectedVehicle) {
    const routeTitle = selectedVehicle.getDisplayName();
    sheetTitle = (
      <div className="flex items-center gap-2">
        <LineBadge routeId={selectedVehicle.routeId} />
        <span className="text-on-surface text-sm font-bold">{routeTitle}</span>
      </div>
    );

    if (selectedVehicle.nextStopArrivalTime !== null) {
      const untilDate = new Date(selectedVehicle.nextStopArrivalTime * 1000);
      const secondsUntil = selectedVehicle.nextStopArrivalTime - Date.now() / 1000;
      const minutes = Math.round(secondsUntil / 60);
      const minutesStr = minutes <= 0 ? null : minutes === 1 ? "1 min" : `${minutes} min`;
      const stopName = selectedVehicle.nextStopId
        ? displayedStops.find((s) => s.ids.includes(selectedVehicle.nextStopId!))?.name
        : null;

      const stopLabel = stopName ? (
        <>
          {" "}
          is <strong>{stopName}</strong>
        </>
      ) : (
        ""
      );
      const timeLabel = minutesStr ? (
        <>
          in{" "}
          <strong>
            <time dateTime={untilDate.toISOString()} title={untilDate.toLocaleString()}>
              {minutesStr}
            </time>
          </strong>
        </>
      ) : (
        <strong>now</strong>
      );

      minimizedBody = (
        <span className="text-on-surface-muted text-xs">
          Next stop{stopLabel} arriving {timeLabel}
        </span>
      );
    }
  } else if (selectedStop) {
    sheetTitle = (
      <span className="text-on-surface truncate text-sm font-bold">{selectedStop.name}</span>
    );

    if (stopBoardFlag && stopDepartures !== undefined && stopDepartures.length > 0) {
      const nextLive = stopDepartures.find((d) => d.kind === "live");
      const next = nextLive ?? stopDepartures[0]!;
      minimizedBody = (
        <DepartureSummary
          departure={next}
          stale={visibleBoards?.some((board) => board.fetchError !== null) ?? false}
        />
      );
    } else if (!stopBoardFlag && stopArrivalTimes !== null) {
      const firstArrival = stopArrivalTimes.find((a) => a.arrivalTime !== null);
      if (firstArrival) {
        const secondsUntil = (firstArrival.arrivalTime!.getTime() - Date.now()) / 1000;
        const minutes = Math.round(secondsUntil / 60);
        const label = minutes <= 0 ? "now" : minutes === 1 ? "1 min" : `${minutes} min`;
        minimizedBody = (
          <span className="text-on-surface-muted text-xs">
            Route {firstArrival.routeId} in {label}
          </span>
        );
      }
    }
  } else if (selectedGbfsStation) {
    sheetTitle = (
      <span className="text-on-surface truncate text-sm font-bold">
        <span className="font-light">[Bajs]</span>&nbsp;
        <span className="capitalize">{selectedGbfsStation.getDisplayName().toLowerCase()}</span>
      </span>
    );
    const bikes = selectedGbfsStation.numBikesAvailable ?? 0;
    minimizedBody = (
      <span className="text-on-surface-muted text-xs">
        {bikes} {bikes === 1 ? "bike" : "bikes"} available
        {selectedGbfsStation.isRenting ? "" : " · not renting"}
      </span>
    );
  }

  useEffect(() => {
    if (!(PLAUSIBLE_SCRIPT_URL && PLAUSIBLE_API_URL && PLAUSIBLE_SITE_URL)) {
      return;
    }

    const script = document.createElement("script");
    script.async = true;
    script.defer = true;
    script.src = PLAUSIBLE_SCRIPT_URL;
    script.dataset.domain = new URL(PLAUSIBLE_SITE_URL).hostname;
    script.dataset.api = PLAUSIBLE_API_URL;

    document.head.appendChild(script);
  }, []);

  return (
    <>
      <>
        <title>{documentTitle}</title>
        <meta property="og:title" content={documentTitle} />
        <link rel="apple-touch-icon" sizes="180x180" href={imgAppleTouchIcon} />
        <link rel="icon" type="image/png" sizes="16x16" href={imgFavicon16x16} />
        <link rel="icon" type="image/png" sizes="32x32" href={imgFavicon32x32} />
        <link rel="icon" type="image/svg+xml" href={imgFaviconSvg} />
      </>
      <main className="h-full">
        <h1 className="sr-only">ZET Live — {documentSubject}</h1>
        <LoadingScreen />
        <Suspense fallback={null}>
          <MapContainer />
        </Suspense>
        <BottomSheet
          open={isOpen}
          title={sheetTitle}
          onClose={clearSelection}
          minimizedBody={minimizedBody}
        >
          {selectedVehicle ? (
            <VehicleSheet
              vehicle={selectedVehicle}
              displayedStops={displayedStops}
              nextStopIndex={nextStopIndex}
              tripStopTimes={tripStopTimes}
              tripFetchError={tripFetchError}
              followEnabled={followEnabled}
              onToggleFollow={() => {
                setFollowEnabled(!followEnabled);
              }}
              onLocate={() => {
                if (selectedVehicle) {
                  useStore.setState({
                    flyToTarget: {
                      longitude: selectedVehicle.lng,
                      latitude: selectedVehicle.lat,
                    },
                  });
                }
              }}
              onStopClick={selectStop}
            />
          ) : selectedStop ? (
            <div className="flex max-h-full flex-col">
              <div className="min-h-0 flex-1 overflow-y-auto">
                {stopBoardFlag && stopSelection ? (
                  <GroupedStopDepartures
                    stopSelection={stopSelection}
                    onVehicleClick={(vehicleId, tripId) => {
                      selectVehicle(vehicleId, tripId, true);
                    }}
                  />
                ) : (
                  <StopSheet
                    stop={selectedStop}
                    arrivals={stopArrivalTimes}
                    onArrivalClick={(vehicleId, tripId) => {
                      selectVehicle(vehicleId, tripId, true);
                    }}
                  />
                )}
              </div>
              <SheetActions actions={stopActions} />
            </div>
          ) : selectedGbfsStation ? (
            <GbfsStationSheet station={selectedGbfsStation} />
          ) : null}
        </BottomSheet>

        <div className="pointer-events-none absolute top-2 right-12 left-2 z-998 grid grid-cols-[minmax(0,auto)_1fr] gap-2">
          <div className="pointer-events-none flex flex-col gap-2 *:pointer-events-auto">
            <SettingsButton />
            <FeedbackButton />
            <AuthButton />
            <div className="h-4">
              <div className="absolute ml-1.5">
                <StatusBar />
              </div>
            </div>
          </div>

          <div className="pointer-events-none flex flex-col gap-2 *:pointer-events-auto">
            <SearchBar />
            <NoticeBar />
          </div>
        </div>
      </main>

      <Toaster position="top-center" />
      <SettingsModal />
      <FeedbackModal />
      <AuthModal />
    </>
  );
}
