import type { StopData } from "@/app/entity/shared";

export class StopV1 {
  id: string;
  name: string;
  lat: number;
  lng: number;
  parentStation: string | null;
  bearing: number | null;

  public constructor(data: StopData) {
    this.id = data.id;
    this.name = data.name;
    this.lat = data.lat;
    this.lng = data.lng;
    this.parentStation = data.parentStation;
    this.bearing = data.bearing;
  }

  public toJSON() {
    return {
      id: this.id,
      name: this.name,
      latitude: this.lat,
      longitude: this.lng,
    };
  }

  public getMapId() {
    return `stop-${this.id}`;
  }
}
