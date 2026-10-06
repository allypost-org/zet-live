import type { Departure } from "@/app/entity/v1/api";
import { useNow } from "@/hooks/use-now";
import { LineBadge } from "@/components/line-badge";
import { formatClockTime, formatMinutesFromNow } from "@/utils/time";

type Props = {
  departures: Departure[] | null;
  scheduleEnd: Date | null;
  fetchError: string | null;
  lastUpdated: number | null;
  onVehicleClick: (vehicleId: string, tripId: string) => void;
  preview?: boolean;
};

/** Countdown/delay threshold: below this many seconds of |delay| a live row
 * counts as on time and shows no strike-through. */
const DELAY_THRESHOLD_MS = 60_000;
/** Scheduled rows further away than this show a clock time instead of a countdown. */
const COUNTDOWN_HORIZON_MS = 60 * 60_000;

export function DepartureSummary({ departure, stale }: { departure: Departure; stale: boolean }) {
  const now = useNow();
  return (
    <span className="text-on-surface-muted text-xs">
      {stale ? "Updates unavailable · " : ""}
      {departureSummary(departure, now)}
    </span>
  );
}

/** Short label for the minimized sheet body: "6 · Črnomerec · 3 min". */
export function departureSummary(d: Departure, now: number): string {
  const time = d.kind === "live" ? d.predictedTime : d.scheduledTime;
  const minutes = Math.round((time.getTime() - now) / 60_000);
  const eta = minutes <= 0 ? "now" : minutes === 1 ? "1 min" : `${minutes} min`;
  return `${d.routeId} · ${d.headsign} · ${eta}`;
}

function LiveDelay({ scheduledTime, predictedTime }: { scheduledTime: Date; predictedTime: Date }) {
  const delayMs = predictedTime.getTime() - scheduledTime.getTime();
  if (Math.abs(delayMs) < DELAY_THRESHOLD_MS) return null;
  const late = delayMs > 0;
  return (
    <span
      className={`col-span-2 col-start-4 grid grid-cols-subgrid items-center text-right whitespace-nowrap tabular-nums ${late ? "text-warn" : "text-primary"}`}
    >
      <s className="text-on-surface-faint">{formatClockTime(scheduledTime)}</s>{" "}
      <span>{formatClockTime(predictedTime)}</span>
    </span>
  );
}

function Row({
  d,
  now,
  onVehicleClick,
}: {
  d: Departure;
  now: number;
  onVehicleClick: Props["onVehicleClick"];
}) {
  const isLive = d.kind === "live";
  const time = isLive ? d.predictedTime : d.scheduledTime;
  const showCountdown = time.getTime() - now < COUNTDOWN_HORIZON_MS;

  if (isLive) {
    const delayMs = d.predictedTime.getTime() - d.scheduledTime.getTime();
    const delayed = Math.abs(delayMs) >= DELAY_THRESHOLD_MS;
    const etaClass = delayed ? (delayMs > 0 ? "text-warn" : "text-primary") : "text-success";
    return (
      <button
        type="button"
        className="bg-surface-dim active:bg-surface-hover col-span-full grid grid-cols-subgrid items-center rounded-lg px-2 py-1.5 text-left"
        onClick={() => {
          onVehicleClick(d.vehicleId, d.tripId);
        }}
      >
        <LineBadge routeId={d.routeId} className="justify-center justify-self-stretch" />
        <span className="bg-success h-1.5 w-1.5 shrink-0 animate-pulse rounded-full" />
        <span className="text-on-surface col-start-3 min-w-0 truncate">{d.headsign}</span>
        {delayed ? (
          <LiveDelay scheduledTime={d.scheduledTime} predictedTime={d.predictedTime} />
        ) : (
          <span className="text-on-surface-faint col-start-5 text-right whitespace-nowrap tabular-nums">
            {formatClockTime(time)}
          </span>
        )}
        <span
          className={`col-start-6 text-right font-bold whitespace-nowrap tabular-nums ${etaClass}`}
        >
          {formatMinutesFromNow(time)}
        </span>
      </button>
    );
  }

  return (
    <div className="col-span-full grid grid-cols-subgrid items-center px-2 py-1.5">
      <LineBadge routeId={d.routeId} className="justify-center justify-self-stretch" />
      <span className="text-on-surface col-start-3 min-w-0 truncate">{d.headsign}</span>
      {showCountdown ? (
        <>
          <span className="text-on-surface-faint col-start-5 text-right whitespace-nowrap tabular-nums">
            {formatClockTime(time)}
          </span>
          <span className="text-on-surface-muted col-start-6 text-right font-medium whitespace-nowrap tabular-nums">
            {formatMinutesFromNow(time)}
          </span>
        </>
      ) : (
        <span className="text-on-surface-muted col-start-5 text-right font-medium whitespace-nowrap tabular-nums">
          {formatClockTime(time)}
        </span>
      )}
    </div>
  );
}

function SectionHeader({ label }: { label: string }) {
  return (
    <div className="text-on-surface-faint col-span-full px-2 text-xs font-bold tracking-wide uppercase [*+&]:mt-4">
      {label}
    </div>
  );
}

export function DeparturesBoard({
  departures,
  scheduleEnd,
  fetchError,
  lastUpdated,
  onVehicleClick,
  preview = false,
}: Props) {
  const now = useNow();

  if (departures === null && fetchError === null) {
    return (
      <div className="mb-2 space-y-2 px-4 text-sm">
        {[0, 1, 2].map((i) => (
          <div key={i} className="bg-surface-dim h-8 animate-pulse rounded-lg" />
        ))}
      </div>
    );
  }

  if (fetchError !== null && (departures === null || departures.length === 0)) {
    return (
      <div className="mb-2 space-y-2 px-4 text-sm">
        <p role="status" className="text-on-surface-faint italic">
          {lastUpdated !== null
            ? `Updates unavailable — last updated ${formatClockTime(new Date(lastUpdated))}. `
            : ""}
          {fetchError}
        </p>
      </div>
    );
  }

  if (departures !== null && departures.length === 0) {
    const scheduleGone = scheduleEnd !== null && now > scheduleEnd.getTime();
    return (
      <div className="mb-2 space-y-2 px-4 text-sm">
        <span className="text-on-surface-faint italic">
          {scheduleGone ? "Schedule unavailable" : "No more departures today"}
        </span>
      </div>
    );
  }

  const live = departures?.filter((d) => d.kind === "live") ?? [];
  const scheduled = departures?.filter((d) => d.kind !== "live") ?? [];

  const row = (d: Departure) => (
    <Row
      key={`${d.kind === "live" ? "v" : "t"}-${d.tripId}-${d.scheduledTime.getTime()}`}
      d={d}
      now={now}
      onVehicleClick={onVehicleClick}
    />
  );

  return (
    <div className="grid grid-cols-[max-content_0.375rem_minmax(0,1fr)_max-content_max-content_max-content] gap-x-2 gap-y-0.5 px-2 text-sm">
      {fetchError !== null ? (
        <p role="status" className="text-warn col-span-full px-2 py-1">
          Updates unavailable
          {lastUpdated !== null
            ? ` — last updated ${formatClockTime(new Date(lastUpdated))}`
            : ""}. {fetchError}
        </p>
      ) : null}
      {preview ? departures?.map(row) : null}
      {!preview && live.length > 0 ? (
        <>
          <SectionHeader label={fetchError === null ? "Live now" : "Last known live arrivals"} />
          {live.map(row)}
        </>
      ) : null}
      {!preview && scheduled.length > 0 ? (
        <>
          <SectionHeader label="Scheduled" />
          {scheduled.map(row)}
        </>
      ) : null}
    </div>
  );
}
