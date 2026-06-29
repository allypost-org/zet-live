import { useEffect, useMemo } from "react";
import { toast } from "sonner";
import type { VehicleV1 } from "@/app/entity/v1/vehicle";
import type { TripStopTimeEntry } from "@/app/trip-stop-times";
import { buildArrivalTimeLookup, lookupStopArrivalTime } from "@/app/trip-stop-times";
import {
  appRequestIdleCallback,
  appRequestAnimationFrame,
  cancelAnimationOrIdleCallback,
} from "@/utils/polyfill/requestSomeCallback";
import { formatMinutesFromNow } from "@/utils/time";

type Props = {
  vehicle: VehicleV1;
  displayedStops: { name: string; ids: string[]; stopSequence?: number }[];
  nextStopIndex: number;
  tripStopTimes: TripStopTimeEntry[] | null;
  tripFetchError: string | null;
  followEnabled: boolean;
  onToggleFollow: () => void;
  onLocate: () => void;
  onStopClick: (stopIds: string[]) => void;
};

export function VehicleSheet({
  vehicle,
  displayedStops,
  nextStopIndex,
  tripStopTimes,
  tripFetchError,
  followEnabled,
  onToggleFollow,
  onLocate,
  onStopClick,
}: Props) {
  const stopBadges = useMemo(() => {
    const lookup = buildArrivalTimeLookup(tripStopTimes);

    return displayedStops.map((stop, i) => {
      const isNext = i === nextStopIndex;
      const isUpcoming = nextStopIndex >= 0 && i > nextStopIndex;

      if (!isNext && !isUpcoming) return null;

      const arrivalTime =
        isNext && vehicle.nextStopArrivalTime !== null
          ? vehicle.nextStopArrivalTime
          : lookupStopArrivalTime(lookup, stop.ids, stop.stopSequence);

      if (arrivalTime === null) return null;

      const at = new Date(arrivalTime * 1000);

      return {
        iso: at.toISOString(),
        locale: at.toLocaleString(),
        fromNow: formatMinutesFromNow(at),
      };
    });
  }, [displayedStops, nextStopIndex, tripStopTimes, vehicle.nextStopArrivalTime]);

  useEffect(() => {
    let timeout = appRequestIdleCallback(() => {
      timeout = appRequestAnimationFrame(() => {
        const $el = document.querySelector<HTMLElement>(
          `.bottom-vehicle-stop-list [data-is-next="true"]`,
        );
        if (!$el) {
          return;
        }
        $el.scrollIntoView({
          behavior: "smooth",
          block: "center",
        });
      });
    });

    return () => {
      cancelAnimationOrIdleCallback(timeout);
    };
  }, [vehicle.id]);

  return (
    <div className="flex max-h-full flex-col">
      <div className="flex max-h-full flex-col gap-2 overflow-y-auto px-4 pb-3">
        {tripFetchError ? (
          <div className="mx-auto flex w-full items-center justify-center gap-1.5 rounded bg-amber-100 px-2.5 py-1.5 text-xs text-amber-800 dark:bg-amber-900/30 dark:text-amber-200">
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
              className="shrink-0"
            >
              <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
              <line x1="12" y1="9" x2="12" y2="13" />
              <line x1="12" y1="17" x2="12.01" y2="17" />
            </svg>
            <span className="whitespace-pre-wrap">{tripFetchError}</span>
          </div>
        ) : null}
        <ul className="bottom-vehicle-stop-list space-y-0.5">
          {displayedStops.map((stop, i) => {
            const isNext = i === nextStopIndex;
            const isPassed = nextStopIndex >= 0 && i < nextStopIndex;
            const arrivalTime = stopBadges[i];

            return (
              <li
                key={`${stop.name}-${i}`}
                data-is-next={isNext ? "true" : undefined}
                className={`hover:bg-surface-hover flex cursor-pointer items-center gap-2 rounded px-2 py-1 text-sm transition-colors duration-300 hover:transition-none ${
                  isNext
                    ? "bg-primary-container text-on-primary-container font-bold"
                    : isPassed
                      ? "text-on-surface-faint"
                      : "text-on-surface-variant"
                }`}
                onClick={() => {
                  onStopClick(stop.ids);
                }}
              >
                <span className="flex shrink-0 items-center justify-center">
                  {isNext ? (
                    <span className="bg-primary inline-block h-2 w-2 rounded-full" />
                  ) : isPassed ? (
                    <span className="bg-on-surface-faint inline-block h-1 w-1 rounded-full" />
                  ) : (
                    <span className="bg-on-surface-muted inline-block h-1.5 w-1.5 rounded-full" />
                  )}
                </span>
                <span className="min-w-0 truncate">{stop.name}</span>
                {arrivalTime && (
                  <time
                    dateTime={arrivalTime.iso}
                    title={arrivalTime.locale}
                    className={`ml-auto shrink-0 rounded px-1.5 py-0.5 text-xs font-bold ${
                      isNext ? "bg-primary text-on-primary" : "bg-surface-dim text-on-surface-muted"
                    }`}
                  >
                    {arrivalTime.fromNow}
                  </time>
                )}
              </li>
            );
          })}
        </ul>
      </div>
      <div className="border-outline flex shrink-0 items-center gap-2 border-t px-4 py-2">
        <label className="text-on-surface-muted flex cursor-pointer items-center gap-2 text-xs font-semibold select-none">
          Follow
          <button
            role="switch"
            aria-checked={followEnabled}
            type="button"
            onClick={onToggleFollow}
            className={`relative inline-flex h-5 w-9 shrink-0 rounded-full transition-colors ${followEnabled ? "bg-primary" : "bg-on-surface-faint"}`}
          >
            <span
              className={`bg-surface inline-block h-5 w-5 rounded-full shadow-sm transition-transform ${followEnabled ? "translate-x-4" : "translate-x-0"}`}
            />
          </button>
        </label>
        <div className="ml-auto flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => {
              const params = new URLSearchParams({ vehicle: vehicle.id });
              if (vehicle.tripId) params.set("trip", vehicle.tripId);
              const url = `${location.origin}${location.pathname}?${params}`;
              if (navigator.share) {
                const shareTitle = `[${vehicle.routeId}] ${vehicle.getDisplayName()}`;

                navigator.share({ title: shareTitle, url }).catch(() => {});
              } else {
                navigator.clipboard.writeText(url).then(
                  () =>
                    toast.success("Link copied to clipboard", {
                      dismissible: true,
                    }),
                  () => {},
                );
              }
            }}
            className="bg-surface-dim text-on-surface-muted hover:bg-surface-hover flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-semibold transition-colors"
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
              <path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8" />
              <polyline points="16 6 12 2 8 6" />
              <line x1="12" y1="2" x2="12" y2="15" />
            </svg>
            Share
          </button>
          <button
            type="button"
            onClick={onLocate}
            className="bg-surface-dim text-on-surface-muted hover:bg-surface-hover flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-semibold transition-colors"
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
              <circle cx="12" cy="12" r="10" />
              <polygon points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76" />
            </svg>
            Center
          </button>
        </div>
      </div>
    </div>
  );
}
