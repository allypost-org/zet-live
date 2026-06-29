import { MapContainer } from "@/components/map-container";
import { BottomSheet } from "@/components/bottom-sheet";
import { StopSheet } from "@/components/stop-sheet";
import { VehicleSheet } from "@/components/vehicle-sheet";
import { GbfsStationSheet } from "@/components/gbfs-station-sheet";
import { StatusBar } from "@/components/status-bar";
import { SearchBar } from "@/components/search-bar";
import { LoadingScreen } from "@/components/loading-screen";
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
import { useEffect, type ReactNode } from "react";
import { SettingsButton, SettingsModal } from "./components/settings-modal";
import { FeedbackButton, FeedbackModal } from "./components/feedback-modal";
import { AuthButton } from "./components/auth-button";
import { AuthModal } from "./components/auth-modal";
import { NoticeBar } from "./components/notice-bar";
import { useWakeLock } from "@/hooks/use-wake-lock";
import { useSetting } from "./settings";
import { PLAUSIBLE_API_URL, PLAUSIBLE_SCRIPT_URL, PLAUSIBLE_SITE_URL } from "./app/consts";
import imgAppleTouchIcon from "@/assets/img/favicon/apple-touch-icon.png";
import imgFavicon16x16 from "@/assets/img/favicon/favicon-16x16.png";
import imgFavicon32x32 from "@/assets/img/favicon/favicon-32x32.png";
import imgFaviconSvg from "@/assets/img/favicon/favicon.svg";

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
  const tripFetchError = vehicleSelection?.fetchError ?? null;
  const followEnabled = vehicleSelection?.followEnabled ?? false;

  const nextStopIndex = selectedVehicle
    ? findNextStopIndex(
        displayedStops,
        selectedVehicle.nextStopSequence,
        selectedVehicle.nextStopId,
      )
    : -1;

  const isOpen = selectedVehicle !== null || selectedStop !== null || selectedGbfsStation !== null;

  let sheetTitle: ReactNode = null;
  let minimizedBody: ReactNode | undefined;

  if (selectedVehicle) {
    const routeTitle = selectedVehicle.getDisplayName();
    const isBus = selectedVehicle.routeId.length > 2;
    sheetTitle = (
      <div className="flex items-center gap-2">
        <span
          className={`text-on-primary inline-flex items-center rounded px-1.5 py-0.5 text-xs font-bold ${isBus ? "bg-primary" : "bg-danger"}`}
        >
          {selectedVehicle.routeId}
        </span>
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

    const firstArrival = stopArrivalTimes?.find((a) => a.arrivalTime !== null);
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
        <link rel="apple-touch-icon" sizes="180x180" href={imgAppleTouchIcon} />
        <link rel="icon" type="image/png" sizes="16x16" href={imgFavicon16x16} />
        <link rel="icon" type="image/png" sizes="32x32" href={imgFavicon32x32} />
        <link rel="icon" type="image/svg+xml" href={imgFaviconSvg} />
      </>
      <LoadingScreen />
      <MapContainer />
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
          <StopSheet
            stop={selectedStop}
            arrivals={stopArrivalTimes}
            onArrivalClick={(vehicleId, tripId) => {
              selectVehicle(vehicleId, tripId, true);
            }}
          />
        ) : selectedGbfsStation ? (
          <GbfsStationSheet station={selectedGbfsStation} />
        ) : null}
      </BottomSheet>

      <div className="pointer-events-none absolute top-2 right-12 left-2 z-1000 grid grid-cols-[minmax(0,auto)_1fr] gap-2 *:pointer-events-auto">
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

      <Toaster position="top-center" />
      <SettingsModal />
      <FeedbackModal />
      <AuthModal />
    </>
  );
}
