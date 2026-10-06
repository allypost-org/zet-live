import { LineBadge } from "@/components/line-badge";
import type { StopArrivalTime } from "@/store";
import { formatMinutesFromNow } from "@/utils/time";

type Props = {
  stop: { name: string; ids: string[]; routes: string[] };
  arrivals: StopArrivalTime[] | null;
  onArrivalClick: (vehicleId: string, tripId: string) => void;
};

export function StopSheet({ arrivals, onArrivalClick }: Props) {
  const grouped = new Map<string, StopArrivalTime[]>();
  for (const a of arrivals ?? []) {
    const list = grouped.get(a.routeId) ?? [];
    list.push(a);
    grouped.set(a.routeId, list);
  }

  for (const list of grouped.values()) {
    list.sort((a, b) => {
      if (a.arrivalTime === null && b.arrivalTime === null) return 0;
      if (a.arrivalTime === null) return 1;
      if (b.arrivalTime === null) return -1;
      return a.arrivalTime.getTime() - b.arrivalTime.getTime();
    });
  }

  const sortedGroups = [...grouped.entries()].sort(([, a], [, b]) => {
    const aMin = a.find((x) => x.arrivalTime !== null)?.arrivalTime?.getTime() ?? Infinity;
    const bMin = b.find((x) => x.arrivalTime !== null)?.arrivalTime?.getTime() ?? Infinity;
    return aMin - bMin;
  });

  return (
    <div className="px-4 pb-3">
      <div className="space-y-1">
        {arrivals === null ? (
          <span className="text-on-surface-faint text-xs italic">Loading arrivals...</span>
        ) : sortedGroups.length === 0 ? (
          <span className="text-on-surface-faint text-xs italic">No active vehicles</span>
        ) : (
          sortedGroups.map(([routeId, times]) => {
            return (
              <div key={routeId} className="flex items-center gap-2">
                <LineBadge routeId={routeId} />
                <div className="flex flex-wrap gap-1">
                  {times.map((t) =>
                    t.arrivalTime !== null ? (
                      <span
                        key={t.vehicleId}
                        className="bg-surface-dim text-on-surface-variant active:bg-surface-hover cursor-pointer rounded px-1.5 py-0.5 text-xs font-medium"
                        onClick={() => {
                          onArrivalClick(t.vehicleId, t.tripId);
                        }}
                      >
                        {formatMinutesFromNow(t.arrivalTime)}
                      </span>
                    ) : null,
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
