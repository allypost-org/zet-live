import type { V1Message } from "./v1/message";

export type StopData = {
  id: string;
  name: string;
  lat: number;
  lng: number;
  parentStation: string | null;
  /** Compass bearing of travel at the stop in degrees, 0 is north. */
  bearing: number | null;
};

export type GroupedStop = {
  name: string;
  lat: number;
  lng: number;
  ids: string[];
  stopSequence?: number;
};

export type StopsUpdateResponse = {
  type: "stops-update";
  stops?: StopData[];
  bounds?: [[number, number], [number, number]];
  grouped: GroupedStop[];
  groupedAll?: GroupedStop[];
  activeStopIds?: string[];
};

export type ProcessedMessageResponse = {
  type: "processed-message";
  data: V1Message;
};

export type WorkerResponse =
  | ProcessedMessageResponse
  //
  | StopsUpdateResponse;
