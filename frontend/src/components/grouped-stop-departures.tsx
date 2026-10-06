import { useLayoutEffect, useRef } from "react";
import { boardingLocationLabel } from "@/app/boarding-location";
import { DeparturesBoard } from "@/components/departures-board";
import { LineBadge } from "@/components/line-badge";
import { useStore, type StopDepartureBoard, type StopSelection } from "@/store";

type Props = {
  stopSelection: StopSelection;
  onVehicleClick: (vehicleId: string, tripId: string) => void;
};

const PREVIEW_DEPARTURES = 3;

function BoardingLocationHeading({
  board,
  label,
  compact,
}: {
  board: StopDepartureBoard;
  label: string;
  compact: boolean;
}) {
  const focusStop = useStore((s) => s.focusStop);
  const destinations = compact ? board.destinations.slice(0, 2) : board.destinations;
  return (
    <div
      className={"flex items-start gap-2 px-4" + (compact ? " cursor-pointer" : "")}
      role={compact ? "button" : undefined}
      aria-label={compact ? `View all departures for boarding location ${label}` : undefined}
      onClick={
        compact
          ? () => {
              focusStop(board.stopId);
            }
          : undefined
      }
    >
      <span className="bg-primary-container text-primary flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-bold">
        {label}
      </span>
      <div className="min-w-0 space-y-1">
        <h2 className="text-on-surface text-sm font-bold">
          {board.destinations.length > 0
            ? `${destinations.join(" · ")}${compact && board.destinations.length > 2 ? ` + ${board.destinations.length - 2}` : ""}`
            : `Boarding location ${label}`}
        </h2>
        {board.routes.length > 1 ? (
          <div className="flex flex-wrap gap-1">
            {board.routes.map((routeId) => (
              <LineBadge key={routeId} routeId={routeId} />
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

export function GroupedStopDepartures({ stopSelection, onVehicleClick }: Props) {
  const rootRef = useRef<HTMLDivElement>(null);
  const focusStop = useStore((s) => s.focusStop);
  const { departureBoards, focusedStopId } = stopSelection;
  const boardIds = departureBoards.map((board) => board.stopId).join(",");
  const grouped = departureBoards.length > 1;
  const boards = departureBoards.filter(
    (board) => focusedStopId === null || board.stopId === focusedStopId,
  );

  useLayoutEffect(() => {
    rootRef.current?.parentElement?.scrollTo({ top: 0 });
  }, [focusedStopId, boardIds]);

  return (
    <div ref={rootRef} className="space-y-4 pb-3">
      {grouped && focusedStopId !== null ? (
        <button
          type="button"
          className="text-primary mb-0 min-h-12 px-4 text-left text-sm font-medium"
          onClick={() => {
            focusStop(null);
          }}
        >
          ‹ All boarding locations
        </button>
      ) : null}
      {boards.map((board) => {
        const label = boardingLocationLabel(
          departureBoards.findIndex((entry) => entry.stopId === board.stopId),
        );
        const preview = grouped && focusedStopId === null;
        const departures = preview
          ? (board.departures
              ?.slice()
              .sort((a, b) => {
                const aTime = a.kind === "live" ? a.predictedTime : a.scheduledTime;
                const bTime = b.kind === "live" ? b.predictedTime : b.scheduledTime;
                return aTime.getTime() - bTime.getTime();
              })
              .slice(0, PREVIEW_DEPARTURES) ?? null)
          : board.departures;

        return (
          <section
            key={board.stopId}
            data-stop-id={board.stopId}
            aria-label={grouped ? `Boarding location ${label}` : "Departures"}
            className="space-y-2"
          >
            {grouped ? (
              <BoardingLocationHeading board={board} label={label} compact={preview} />
            ) : null}
            <DeparturesBoard
              departures={departures}
              scheduleEnd={board.scheduleEnd}
              fetchError={board.fetchError}
              lastUpdated={board.lastUpdated}
              onVehicleClick={onVehicleClick}
              preview={preview}
            />
            {preview ? (
              <button
                type="button"
                aria-label={`View all departures for boarding location ${label}`}
                className="text-primary -mt-4 min-h-12 px-4 text-left text-sm font-medium"
                onClick={() => {
                  focusStop(board.stopId);
                }}
              >
                View all departures ›
              </button>
            ) : null}
          </section>
        );
      })}
    </div>
  );
}
