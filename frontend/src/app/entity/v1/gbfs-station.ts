import { getT } from "@/i18n";

/** Indices match `GbfsStation::to_simple()` in the backend. */
const SIMPLE_FIELD = {
  id: 0,
  name: 1,
  latitude: 2,
  longitude: 3,
  numBikesAvailable: 4,
  numDocksAvailable: 5,
  isRenting: 6,
  isReturning: 7,
  capacity: 8,
} as const;

const SIMPLE_MIN_LENGTH = 4;

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

export class GbfsStationV1 {
  id: string;
  name: string | null;
  lat: number;
  lng: number;
  numBikesAvailable: number | null;
  numDocksAvailable: number | null;
  isRenting: boolean;
  isReturning: boolean;
  capacity: number | null;

  public constructor(data: {
    id: string;
    name?: string | null;
    latitude: number;
    longitude: number;
    numBikesAvailable?: number | null;
    numDocksAvailable?: number | null;
    isRenting?: boolean;
    isReturning?: boolean;
    capacity?: number | null;
  }) {
    this.id = data.id;
    this.name = data.name ?? null;
    this.lat = data.latitude;
    this.lng = data.longitude;
    this.numBikesAvailable = data.numBikesAvailable ?? null;
    this.numDocksAvailable = data.numDocksAvailable ?? null;
    this.isRenting = data.isRenting ?? true;
    this.isReturning = data.isReturning ?? true;
    this.capacity = data.capacity ?? null;
  }

  public static fromSimple(data: (string | number)[]) {
    if (data.length < SIMPLE_MIN_LENGTH) {
      throw new Error(`GBFS station simple array needs at least ${SIMPLE_MIN_LENGTH} elements`);
    }

    return new GbfsStationV1({
      id: String(data[SIMPLE_FIELD.id]),
      name: optionalString(data, SIMPLE_FIELD.name),
      latitude: Number(data[SIMPLE_FIELD.latitude]),
      longitude: Number(data[SIMPLE_FIELD.longitude]),
      numBikesAvailable: optionalNumber(data, SIMPLE_FIELD.numBikesAvailable),
      numDocksAvailable: optionalNumber(data, SIMPLE_FIELD.numDocksAvailable),
      isRenting: optionalNumber(data, SIMPLE_FIELD.isRenting) !== 0,
      isReturning: optionalNumber(data, SIMPLE_FIELD.isReturning) !== 0,
      capacity: optionalNumber(data, SIMPLE_FIELD.capacity),
    });
  }

  public getDisplayName(): string {
    return this.name?.trim() || getT()("entity.stationFallback", { id: this.id });
  }

  public getMapId() {
    return `gbfs-station-${this.id}`;
  }
}
