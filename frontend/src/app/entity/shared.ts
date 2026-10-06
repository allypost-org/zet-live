import type { V1Message } from "./v1/message";

export type StopData = {
  id: string;
  name: string;
  lat: number;
  lng: number;
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
