import { getT, type TFn } from "@/i18n";

/** Indices match `Vehicle::to_simple()` in the backend. */
const SIMPLE_FIELD = {
  id: 0,
  routeId: 1,
  tripId: 2,
  latitude: 3,
  longitude: 4,
  prevLat: 5,
  prevLng: 6,
  nextStopId: 7,
  nextStopSequence: 8,
  nextStopArrivalDelay: 9,
  nextStopArrivalTime: 10,
  bearing: 11,
  routeLongName: 12,
  tripHeadsign: 13,
} as const;

const SIMPLE_MIN_LENGTH = 5;

function optionalString(data: (string | number)[], index: number): string | null {
  if (index >= data.length) return null;
  const value = data[index];
  if (value === null) return null;
  return String(value);
}

function optionalNumber(data: (string | number)[], index: number): number | null {
  if (index >= data.length) return null;
  const value = data[index];
  if (value === null) return null;
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

export class VehicleV1 {
  id: string;
  routeId: string;
  routeLongName: string | null;
  tripHeadsign: string | null;
  tripId: string;
  lat: number;
  lng: number;
  prevLat: number | null;
  prevLng: number | null;
  bearing: number | null;
  nextStopId: string | null;
  nextStopSequence: number | null;
  nextStopArrivalDelay: number | null;
  nextStopArrivalTime: number | null;

  public constructor(data: {
    id: string;
    routeId: string;
    routeLongName?: string | null;
    tripHeadsign?: string | null;
    tripId: string;
    latitude: number;
    longitude: number;
    prevLat?: number | null;
    prevLng?: number | null;
    bearing?: number | null;
    nextStopId?: string | null;
    nextStopSequence?: number | null;
    nextStopArrivalDelay?: number | null;
    nextStopArrivalTime?: number | null;
  }) {
    this.id = data.id;
    this.routeId = data.routeId;
    this.routeLongName = data.routeLongName ?? null;
    this.tripHeadsign = data.tripHeadsign ?? null;
    this.tripId = data.tripId;
    this.lat = data.latitude;
    this.lng = data.longitude;
    this.prevLat = data.prevLat ?? null;
    this.prevLng = data.prevLng ?? null;
    this.bearing = data.bearing ?? null;
    this.nextStopId = data.nextStopId ?? null;
    this.nextStopSequence = data.nextStopSequence ?? null;
    this.nextStopArrivalDelay = data.nextStopArrivalDelay ?? null;
    this.nextStopArrivalTime = data.nextStopArrivalTime ?? null;
  }

  public static fromSimple(data: (string | number)[]) {
    if (data.length < SIMPLE_MIN_LENGTH) {
      throw new Error(`Vehicle simple array needs at least ${SIMPLE_MIN_LENGTH} elements`);
    }

    return new VehicleV1({
      id: String(data[SIMPLE_FIELD.id]),
      routeId: String(data[SIMPLE_FIELD.routeId]),
      tripId: String(data[SIMPLE_FIELD.tripId]),
      latitude: Number(data[SIMPLE_FIELD.latitude]),
      longitude: Number(data[SIMPLE_FIELD.longitude]),
      prevLat: optionalNumber(data, SIMPLE_FIELD.prevLat),
      prevLng: optionalNumber(data, SIMPLE_FIELD.prevLng),
      bearing: optionalNumber(data, SIMPLE_FIELD.bearing),
      nextStopId: optionalString(data, SIMPLE_FIELD.nextStopId),
      nextStopSequence: optionalNumber(data, SIMPLE_FIELD.nextStopSequence),
      nextStopArrivalDelay: optionalNumber(data, SIMPLE_FIELD.nextStopArrivalDelay),
      nextStopArrivalTime: optionalNumber(data, SIMPLE_FIELD.nextStopArrivalTime),
      routeLongName: optionalString(data, SIMPLE_FIELD.routeLongName),
      tripHeadsign: optionalString(data, SIMPLE_FIELD.tripHeadsign),
    });
  }

  public toJSON() {
    return {
      id: this.id,
      routeId: this.routeId,
      routeLongName: this.routeLongName,
      tripHeadsign: this.tripHeadsign,
      tripId: this.tripId,
      latitude: this.lat,
      longitude: this.lng,
      prevLat: this.prevLat,
      prevLng: this.prevLng,
      bearing: this.bearing,
      nextStopId: this.nextStopId,
      nextStopSequence: this.nextStopSequence,
      nextStopArrivalDelay: this.nextStopArrivalDelay,
      nextStopArrivalTime: this.nextStopArrivalTime,
    };
  }

  public getDisplayName(t: TFn = getT()): string {
    return (
      this.tripHeadsign?.trim() ||
      this.routeLongName?.trim() ||
      t("entity.routeFallback", { id: this.routeId })
    );
  }

  public getMapId() {
    return `vehicle-${this.id}`;
  }
}
