import { getT, type TFn } from "@/i18n";
import type { LngLat, VehicleData } from "./vehicle-data";

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
  /** Route points between the previous and the current position. */
  path: LngLat[];

  public constructor(data: VehicleData) {
    this.id = data.id;
    this.routeId = data.routeId;
    this.routeLongName = data.routeLongName;
    this.tripHeadsign = data.tripHeadsign;
    this.tripId = data.tripId;
    this.lat = data.latitude;
    this.lng = data.longitude;
    this.prevLat = data.prevLat;
    this.prevLng = data.prevLng;
    this.bearing = data.bearing;
    this.nextStopId = data.nextStopId;
    this.nextStopSequence = data.nextStopSequence;
    this.nextStopArrivalDelay = data.nextStopArrivalDelay;
    this.nextStopArrivalTime = data.nextStopArrivalTime;
    this.path = data.path;
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
      path: this.path,
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
