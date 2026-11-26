import { useEffect, useRef } from "react";
import { motion, AnimatePresence } from "motion/react";
import {
  settingsStore,
  updateSetting,
  useSetting,
  type MapStyleId,
  type MapThemeMode,
  type UiThemeMode,
  type UiTheme,
  type VehicleMotion,
} from "@/settings";
import { openFeedback } from "@/feedback-store";
import type { ReactNode } from "react";
import { cn } from "@/utils/style";

const MAP_STYLES: { id: MapStyleId; label: string }[] = [
  { id: "3d", label: "3D" },
  { id: "3d.dark", label: "3D Dark" },
  { id: "flat", label: "Flat" },
  { id: "satellite", label: "Satellite" },
];

const MAP_THEME_MODES: { id: MapThemeMode; label: string; description: string }[] = [
  { id: "device", label: "Device", description: "System preference" },
  { id: "time", label: "Sun Cycle", description: "Sunrise & sunset" },
  { id: "manual", label: "Manual", description: "Choose yourself" },
];

const UI_THEME_MODES: { id: UiThemeMode; label: string; description: string }[] = [
  { id: "device", label: "Device", description: "System preference" },
  { id: "time", label: "Sun Cycle", description: "Sunrise & sunset" },
  { id: "map", label: "Map", description: "Match map style" },
  { id: "manual", label: "Manual", description: "Choose yourself" },
];

const VEHICLE_MOTIONS: { id: VehicleMotion; label: string; description: string }[] = [
  { id: "animate", label: "Animate", description: "Move smoothly when zoomed in" },
  { id: "trail", label: "Trail", description: "Jump and show the last move" },
];

const UI_THEMES: { id: UiTheme; label: string }[] = [
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" },
];

export function SettingsModal() {
  const open = settingsStore((s) => s._settingsOpen);
  const currentStyle = useSetting("mapStyle");
  const mapThemeMode = useSetting("mapThemeMode");
  const wakeLockEnabled = useSetting("wakeLockEnabled");
  const uiThemeMode = useSetting("uiThemeMode");
  const uiThemeManual = useSetting("uiThemeManual");
  const showGbfsStations = useSetting("showGbfsStations");
  const showBuses = useSetting("showBuses");
  const showTrams = useSetting("showTrams");
  const vehicleMotion = useSetting("vehicleMotion");
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        settingsStore.setState({ _settingsOpen: false });
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  return (
    <AnimatePresence>
      {open && (
        <div
          id="settings-panel"
          className="pointer-events-auto fixed inset-0 z-2000 flex items-center justify-center"
        >
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="absolute inset-0 bg-black/30"
            onClick={() => {
              settingsStore.setState({ _settingsOpen: false });
            }}
          />

          <motion.div
            ref={panelRef}
            initial={{ opacity: 0, scale: 0.95, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: 10 }}
            transition={{ type: "spring", damping: 25, stiffness: 300 }}
            className="bg-surface relative z-10 max-h-[85dvh] w-[90vw] max-w-md overflow-auto rounded-2xl shadow-xl"
            aria-label="Settings"
            aria-modal="true"
            aria-expanded="true"
          >
            <div className="bg-surface sticky top-0 flex items-center justify-between rounded-t-2xl px-4 py-2">
              <h2 className="text-on-surface text-base font-bold">Settings</h2>
              <button
                type="button"
                aria-label="Close settings"
                aria-expanded={open}
                aria-controls="settings-panel"
                onClick={() => {
                  settingsStore.setState({ _settingsOpen: false });
                }}
                className="text-on-surface-faint hover:bg-surface-hover hover:text-on-surface-muted rounded-full p-2 transition-colors"
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="18"
                  height="18"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </div>

            <div className="flex flex-col gap-6 px-5 pb-5">
              <SettingsCategory
                title="Appearance"
                sections={[
                  {
                    title: "Theme Source",
                    description: "Control when dark mode is active.",
                    body: (
                      <div className="grid grid-cols-2 gap-2">
                        {UI_THEME_MODES.map((m) => (
                          <ItemSelect
                            key={m.id}
                            selected={uiThemeMode === m.id}
                            title={m.label}
                            description={m.description}
                            onSelect={() => {
                              updateSetting("uiThemeMode", m.id);
                            }}
                          />
                        ))}
                      </div>
                    ),
                  },
                  ...(uiThemeMode === "manual"
                    ? [
                        {
                          title: "Theme",
                          description: "Choose between light and dark appearance.",
                          body: (
                            <div className="grid grid-cols-2 gap-2">
                              {UI_THEMES.map((t) => (
                                <ItemSelect
                                  key={t.id}
                                  selected={uiThemeManual === t.id}
                                  title={t.label}
                                  onSelect={() => {
                                    updateSetting("uiThemeManual", t.id);
                                  }}
                                />
                              ))}
                            </div>
                          ),
                        },
                      ]
                    : []),
                ]}
              />

              <SettingsCategory
                title="Map"
                sections={[
                  {
                    title: "Style Source",
                    description: "Auto-switch between 3D and 3D Dark, or pick a style manually.",
                    body: (
                      <div className="grid grid-cols-3 gap-2">
                        {MAP_THEME_MODES.map((m) => (
                          <ItemSelect
                            key={m.id}
                            selected={mapThemeMode === m.id}
                            title={m.label}
                            description={m.description}
                            onSelect={() => {
                              updateSetting("mapThemeMode", m.id);
                            }}
                          />
                        ))}
                      </div>
                    ),
                  },
                  ...(mapThemeMode === "manual"
                    ? [
                        {
                          title: "Style",
                          description: "Choose the visual style for the map.",
                          body: (
                            <div className="grid grid-cols-2 gap-2">
                              {MAP_STYLES.map((s) => (
                                <ItemSelect
                                  key={s.id}
                                  selected={currentStyle === s.id}
                                  title={s.label}
                                  onSelect={() => {
                                    updateSetting("mapStyle", s.id);
                                  }}
                                  className="text-center"
                                />
                              ))}
                            </div>
                          ),
                        },
                      ]
                    : []),
                  {
                    title: "Displayed items",
                    description: "Which features should be displayed on the map.",
                    body: (
                      <div className="grid grid-cols-2 gap-2">
                        <OnOffSwitch
                          enabled={showBuses}
                          title="Buses"
                          onToggle={() => {
                            updateSetting("showBuses", !showBuses);
                          }}
                        />
                        <OnOffSwitch
                          enabled={showTrams}
                          title="Trams"
                          onToggle={() => {
                            updateSetting("showTrams", !showTrams);
                          }}
                        />
                        <OnOffSwitch
                          enabled={showGbfsStations}
                          title="Bajs Stations"
                          onToggle={() => {
                            updateSetting("showGbfsStations", !showGbfsStations);
                          }}
                        />
                      </div>
                    ),
                  },
                  {
                    title: "Vehicle Movement",
                    description:
                      "How vehicles move to a new position. Zoomed out or with reduced motion, vehicles always jump and show a trail.",
                    body: (
                      <div className="grid grid-cols-2 gap-2">
                        {VEHICLE_MOTIONS.map((m) => (
                          <ItemSelect
                            key={m.id}
                            selected={vehicleMotion === m.id}
                            title={m.label}
                            description={m.description}
                            onSelect={() => {
                              updateSetting("vehicleMotion", m.id);
                            }}
                          />
                        ))}
                      </div>
                    ),
                  },
                ]}
              />

              <SettingsCategory
                title="General"
                sections={[
                  {
                    title: "Keep Screen Awake",
                    description:
                      "Prevent your screen from turning off while tracking transit. Uses the browser Wake Lock API.",
                    body: (
                      <OnOffSwitch
                        enabled={wakeLockEnabled}
                        title={wakeLockEnabled ? "Screen stays on" : "Screen can turn off"}
                        onToggle={() => {
                          updateSetting("wakeLockEnabled", !wakeLockEnabled);
                        }}
                      />
                    ),
                  },
                  {
                    title: "Send Feedback",
                    description: "Report a bug, request a feature, or share a thought.",
                    body: (
                      <button
                        type="button"
                        onClick={() => {
                          settingsStore.setState({ _settingsOpen: false });
                          openFeedback();
                        }}
                        className="border-outline bg-surface text-on-surface-variant hover:bg-surface-hover flex w-full cursor-pointer items-center justify-between rounded-lg border px-3 py-2.5 text-sm font-medium transition-colors"
                      >
                        <span>Open feedback form</span>
                        <svg
                          xmlns="http://www.w3.org/2000/svg"
                          width="16"
                          height="16"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="2"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        >
                          <path d="M5 12h14" />
                          <path d="m12 5 7 7-7 7" />
                        </svg>
                      </button>
                    ),
                  },
                ]}
              />
            </div>
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
}

export function SettingsButton() {
  const open = settingsStore((s) => s._settingsOpen);

  return (
    <button
      type="button"
      aria-label="Open settings"
      onClick={() => {
        settingsStore.setState({ _settingsOpen: true });
      }}
      aria-expanded={open}
      aria-controls="settings-panel"
      className="bg-surface-overlay text-on-surface-variant hover:bg-surface flex h-9 w-9 cursor-pointer items-center justify-center rounded-lg shadow-md backdrop-blur-sm transition-colors"
      title="Settings"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
        <circle cx="12" cy="12" r="3" />
      </svg>
    </button>
  );
}

function OnOffSwitch(props: {
  enabled: boolean;
  title?: string;
  description?: string;
  className?: string;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={props.enabled}
      tabIndex={0}
      onClick={props.onToggle}
      className={cn(
        "flex w-full cursor-pointer items-center justify-between rounded-lg border px-3 py-2.5 text-sm font-medium transition-colors",
        props.enabled
          ? "border-success bg-success-container text-on-success-container"
          : "border-outline bg-surface text-on-surface-variant hover:bg-surface-hover",
        props.className,
      )}
    >
      <div className="flex flex-col gap-1 text-left">
        <span>{props.title ?? (props.enabled ? "Visible" : "Hidden")}</span>
        {props.description ? (
          <span className="text-on-surface-muted text-xs font-normal">{props.description}</span>
        ) : null}
      </div>
      <span
        className={cn(
          "ml-2 rounded-full px-2 py-0.5 text-xs font-semibold",
          props.enabled
            ? "bg-success-container text-on-success-container"
            : "bg-surface-dim text-on-surface-muted",
        )}
      >
        {props.enabled ? "On" : "Off"}
      </span>
    </button>
  );
}

function ItemSelect(props: {
  selected: boolean;
  title: string;
  description?: string;
  onSelect: () => void;
  className?: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={props.selected}
      tabIndex={0}
      onClick={props.onSelect}
      className={cn(
        "cursor-pointer rounded-lg border px-3 py-2 text-left text-sm font-medium transition-colors",
        props.selected
          ? "border-primary bg-primary-container text-on-primary-container"
          : "border-outline bg-surface text-on-surface-variant hover:bg-surface-hover",
        props.className,
      )}
    >
      <span className="block">{props.title}</span>
      {props.description ? (
        <span className="text-on-surface-muted block text-xs font-normal">{props.description}</span>
      ) : null}
    </button>
  );
}

function SettingsCategory(props: {
  title: ReactNode;
  sections: {
    title: ReactNode;
    description: ReactNode;
    body?: ReactNode;
  }[];
}) {
  return (
    <section>
      <h3 className="text-on-surface-muted mb-2 text-xs font-semibold tracking-wide uppercase">
        {props.title}
      </h3>
      <div className="flex flex-col gap-2">
        {props.sections.map((s, i) => (
          <div key={i} className="bg-surface-dim flex flex-col rounded-xl p-3">
            <div className="text-on-surface mb-1 cursor-default text-sm font-medium">{s.title}</div>
            <div className="text-on-surface-muted cursor-default text-xs not-last:mb-3">
              {s.description}
            </div>
            {s.body}
          </div>
        ))}
      </div>
    </section>
  );
}
