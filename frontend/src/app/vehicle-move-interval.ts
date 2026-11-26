/**
 * Estimates the time between two position changes of each vehicle. A vehicle
 * animation that lasts this long ends when the next position arrives.
 *
 * The feed has no timestamp for each position, so the estimate uses the times
 * at which the client sees a position change.
 */

/** Interval to use before any interval is measured. */
export const DEFAULT_MOVE_INTERVAL_MS = 12_000;
const MIN_INTERVAL_MS = 1_000;
const MAX_INTERVAL_MS = 30_000;
/** A longer gap is a long stop or missing data, not the report interval. */
const MAX_MEASURED_GAP_MS = 60_000;
/** Intervals kept for each vehicle. The median ignores one long stop. */
const SAMPLES_PER_VEHICLE = 5;
/** Intervals kept from all vehicles, for vehicles with no measured interval. */
const SAMPLES_ALL_VEHICLES = 50;

type VehicleHistory = {
  lat: number;
  lng: number;
  /** Time of the last seen position change. `null` until the first change. */
  changedAt: number | null;
  intervals: number[];
};

export type MoveIntervalState = {
  vehicles: Map<string, VehicleHistory>;
  recent: number[];
};

export type VehiclePosition = { id: string; lat: number; lng: number };

export function createMoveIntervalState(): MoveIntervalState {
  return { vehicles: new Map(), recent: [] };
}

function pushLimited(values: number[], value: number, limit: number) {
  values.push(value);
  if (values.length > limit) values.splice(0, values.length - limit);
}

function median(values: readonly number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[middle]! : (sorted[middle - 1]! + sorted[middle]!) / 2;
}

/**
 * Records the positions of one vehicle message that arrived at `now` (ms).
 * Returns the expected interval in ms until the next position change of each
 * vehicle in the message. Vehicles that are not in the message are forgotten.
 */
export function observeVehiclePositions(
  state: MoveIntervalState,
  positions: Iterable<VehiclePosition>,
  now: number,
): Map<string, number> {
  const vehicles = new Map<string, VehicleHistory>();
  for (const { id, lat, lng } of positions) {
    const history = state.vehicles.get(id) ?? { lat, lng, changedAt: null, intervals: [] };
    if (history.lat !== lat || history.lng !== lng) {
      if (history.changedAt !== null) {
        const gap = now - history.changedAt;
        if (gap > 0 && gap <= MAX_MEASURED_GAP_MS) {
          pushLimited(history.intervals, gap, SAMPLES_PER_VEHICLE);
          pushLimited(state.recent, gap, SAMPLES_ALL_VEHICLES);
        }
      }
      history.lat = lat;
      history.lng = lng;
      history.changedAt = now;
    }
    vehicles.set(id, history);
  }
  state.vehicles = vehicles;

  const fallback = median(state.recent) ?? DEFAULT_MOVE_INTERVAL_MS;
  const intervals = new Map<string, number>();
  for (const [id, history] of vehicles) {
    const interval = median(history.intervals) ?? fallback;
    intervals.set(id, Math.min(MAX_INTERVAL_MS, Math.max(MIN_INTERVAL_MS, interval)));
  }
  return intervals;
}
