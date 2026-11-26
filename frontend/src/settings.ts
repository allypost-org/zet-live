import { create } from "zustand";
import { z } from "zod";

export type MapStyleId = "3d" | "3d.dark" | "flat" | "satellite";
export type MapThemeMode = "device" | "time" | "manual";
export type UiThemeMode = "device" | "time" | "map" | "manual";
export type UiTheme = "light" | "dark";
/** How vehicles move to a new position: move smoothly, or jump and show a trail. */
export type VehicleMotion = "animate" | "trail";
export type LanguageChoice = "auto" | "en" | "hr";
// "mmm" is the short month name in the interface language.
export const DATE_FORMATS = [
  "dd/mm/yyyy",
  "dd-mm-yyyy",
  "dd.mm.yyyy.",
  "mm/dd/yyyy",
  "yyyy-mm-dd",
  "d/m/yyyy",
  "d-m-yyyy",
  "d.m.yyyy.",
  "m/d/yyyy",
  "d mmm yyyy",
  "d. mmm yyyy.",
  "mmm d, yyyy",
] as const;
export type DateFormat = (typeof DATE_FORMATS)[number];
export const TIME_FORMATS = ["HH:mm", "h:mm a"] as const;
export type TimeFormat = (typeof TIME_FORMATS)[number];

const dateFormatSchema = z.enum(DATE_FORMATS);
const timeFormatSchema = z.enum(TIME_FORMATS);

const DEFAULTS = {
  mapStyle: "3d" as MapStyleId,
  mapThemeMode: "manual" as MapThemeMode,
  wakeLockEnabled: true,
  uiThemeMode: "map" as UiThemeMode,
  uiThemeManual: "light" as UiTheme,
  showGbfsStations: true,
  showBuses: true,
  showTrams: true,
  vehicleMotion: "animate" as VehicleMotion,
  language: "auto" as LanguageChoice,
  dateFormat: "dd/mm/yyyy" as DateFormat,
  timeFormat: "HH:mm" as TimeFormat,
};

export type Settings = typeof DEFAULTS & {
  _settingsOpen: boolean;
};

const STORAGE_KEY = "settings";

function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<Settings>;
      return {
        mapStyle: isValidMapStyle(parsed.mapStyle) ? parsed.mapStyle : DEFAULTS.mapStyle,
        mapThemeMode: isValidMapThemeMode(parsed.mapThemeMode)
          ? parsed.mapThemeMode
          : DEFAULTS.mapThemeMode,
        wakeLockEnabled:
          typeof parsed.wakeLockEnabled === "boolean"
            ? parsed.wakeLockEnabled
            : DEFAULTS.wakeLockEnabled,
        uiThemeMode: isValidUiThemeMode(parsed.uiThemeMode)
          ? parsed.uiThemeMode
          : DEFAULTS.uiThemeMode,
        uiThemeManual: isValidUiTheme(parsed.uiThemeManual)
          ? parsed.uiThemeManual
          : DEFAULTS.uiThemeManual,
        showGbfsStations:
          typeof parsed.showGbfsStations === "boolean"
            ? parsed.showGbfsStations
            : DEFAULTS.showGbfsStations,
        showBuses: typeof parsed.showBuses === "boolean" ? parsed.showBuses : DEFAULTS.showBuses,
        showTrams: typeof parsed.showTrams === "boolean" ? parsed.showTrams : DEFAULTS.showTrams,
        vehicleMotion: isValidVehicleMotion(parsed.vehicleMotion)
          ? parsed.vehicleMotion
          : DEFAULTS.vehicleMotion,
        language: isValidLanguageChoice(parsed.language) ? parsed.language : DEFAULTS.language,
        dateFormat: dateFormatSchema.safeParse(parsed.dateFormat).data ?? DEFAULTS.dateFormat,
        timeFormat: timeFormatSchema.safeParse(parsed.timeFormat).data ?? DEFAULTS.timeFormat,
        _settingsOpen: false,
      };
    }
  } catch (e) {
    console.error("Failed to load settings from localStorage", e);
  }
  return { ...DEFAULTS, _settingsOpen: false };
}

function isValidMapStyle(value: unknown): value is MapStyleId {
  return value === "3d" || value === "3d.dark" || value === "flat" || value === "satellite";
}

function isValidMapThemeMode(value: unknown): value is MapThemeMode {
  return value === "device" || value === "time" || value === "manual";
}

function isValidUiThemeMode(value: unknown): value is UiThemeMode {
  return value === "device" || value === "time" || value === "map" || value === "manual";
}

function isValidUiTheme(value: unknown): value is UiTheme {
  return value === "light" || value === "dark";
}

function isValidVehicleMotion(value: unknown): value is VehicleMotion {
  return value === "animate" || value === "trail";
}

function isValidLanguageChoice(value: unknown): value is LanguageChoice {
  return value === "auto" || value === "en" || value === "hr";
}

function persist(settings: Settings) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch (e) {
    console.error("Failed to persist settings to localStorage", e);
  }
}

export const settingsStore = create<Settings>()(() => loadSettings());

export function useSetting<T extends keyof Settings>(key: T): Settings[T] {
  return settingsStore((s) => s[key]);
}

export function updateSetting<K extends keyof Settings>(key: K, value: Settings[K]) {
  settingsStore.setState({ [key]: value });
  persist(settingsStore.getState());
}

export const SYNC_SETTINGS_KEYS = [
  "mapStyle",
  "mapThemeMode",
  "wakeLockEnabled",
  "uiThemeMode",
  "uiThemeManual",
  "showGbfsStations",
  "showBuses",
  "showTrams",
  "language",
  "dateFormat",
  "timeFormat",
] as const;

export type SyncSettings = Pick<Settings, (typeof SYNC_SETTINGS_KEYS)[number]>;

export const syncSettingsSchema = z.object({
  mapStyle: z.string(),
  mapThemeMode: z.string(),
  wakeLockEnabled: z.boolean(),
  uiThemeMode: z.string(),
  uiThemeManual: z.string(),
  showGbfsStations: z.boolean(),
  showBuses: z.boolean(),
  showTrams: z.boolean(),
  language: z.enum(["auto", "en", "hr"]).default("auto").catch("auto"),
  dateFormat: dateFormatSchema.default(DEFAULTS.dateFormat).catch(DEFAULTS.dateFormat),
  timeFormat: timeFormatSchema.default(DEFAULTS.timeFormat).catch(DEFAULTS.timeFormat),
});

export function serializeSync(state: Settings): SyncSettings {
  return {
    mapStyle: state.mapStyle,
    mapThemeMode: state.mapThemeMode,
    wakeLockEnabled: state.wakeLockEnabled,
    uiThemeMode: state.uiThemeMode,
    uiThemeManual: state.uiThemeManual,
    showGbfsStations: state.showGbfsStations,
    showBuses: state.showBuses,
    showTrams: state.showTrams,
    language: state.language,
    dateFormat: state.dateFormat,
    timeFormat: state.timeFormat,
  };
}

export function getSyncSettings(): SyncSettings {
  return serializeSync(settingsStore.getState());
}

export function applySyncSettings(remote: unknown): void {
  const parsed = syncSettingsSchema.safeParse(remote);
  if (!parsed.success) return;
  settingsStore.setState({ ...parsed.data } as Partial<Settings>);
  persist(settingsStore.getState());
}
