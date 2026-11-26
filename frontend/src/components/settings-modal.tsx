import { useEffect, useRef } from "react";
import { motion, AnimatePresence } from "motion/react";
import {
  settingsStore,
  updateSetting,
  useSetting,
  type LanguageChoice,
  type MapStyleId,
  type MapThemeMode,
  type UiThemeMode,
  type UiTheme,
  type VehicleMotion,
  DATE_FORMATS,
  TIME_FORMATS,
} from "@/settings";
import { openFeedback } from "@/feedback-store";
import type { ReactNode } from "react";
import { cn } from "@/utils/style";
import { useT } from "@/i18n";
import { formatDate, formatTime } from "@/utils/time";

const MAP_STYLES: { id: MapStyleId }[] = [
  { id: "3d" },
  { id: "3d.dark" },
  { id: "flat" },
  { id: "satellite" },
];

const MAP_THEME_MODES: { id: MapThemeMode }[] = [
  { id: "device" },
  { id: "time" },
  { id: "manual" },
];

const UI_THEME_MODES: { id: UiThemeMode }[] = [
  { id: "device" },
  { id: "time" },
  { id: "map" },
  { id: "manual" },
];

const VEHICLE_MOTIONS: { id: VehicleMotion }[] = [{ id: "animate" }, { id: "trail" }];

const UI_THEMES: { id: UiTheme }[] = [{ id: "light" }, { id: "dark" }];

const LANGUAGE_CHOICES: { id: LanguageChoice }[] = [{ id: "auto" }, { id: "en" }, { id: "hr" }];

// Each option shows its format applied to this sample date. Day 21 makes the
// day-month order unambiguous, and 13:45 shows the 12-hour conversion.
const FORMAT_SAMPLE = new Date(2026, 3, 21, 13, 45);

const TIME_FORMAT_OPTIONS = TIME_FORMATS.map((id) => ({
  id,
  label: formatTime(FORMAT_SAMPLE, id),
}));

export function SettingsModal() {
  const t = useT();
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
  const languageChoice = useSetting("language");
  const dateFormat = useSetting("dateFormat");
  const dateFormatOptions = DATE_FORMATS.map((id) => ({
    id,
    label: formatDate(FORMAT_SAMPLE, id, t),
  }));
  const timeFormat = useSetting("timeFormat");
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
            aria-label={t("settings.title")}
            aria-modal="true"
            aria-expanded="true"
          >
            <div className="bg-surface sticky top-0 flex items-center justify-between rounded-t-2xl px-4 py-2">
              <h2 className="text-on-surface text-base font-bold">{t("settings.title")}</h2>
              <button
                type="button"
                aria-label={t("settings.aria.close")}
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
                title={t("settings.categories.language")}
                sections={[
                  {
                    title: t("language.choiceTitle"),
                    description: t("language.description"),
                    body: (
                      <div className="grid grid-cols-3 gap-2">
                        {LANGUAGE_CHOICES.map((c) => (
                          <ItemSelect
                            key={c.id}
                            selected={languageChoice === c.id}
                            title={t(`language.${c.id}`)}
                            description={t(`language.${c.id}Description`)}
                            onSelect={() => {
                              updateSetting("language", c.id);
                            }}
                          />
                        ))}
                      </div>
                    ),
                  },
                  {
                    title: t("settings.format.title"),
                    description: t("settings.format.description"),
                    body: (
                      <div className="grid grid-cols-2 gap-2">
                        <SettingsSelect
                          label={t("settings.format.date")}
                          value={dateFormat}
                          options={dateFormatOptions}
                          onChange={(value) => {
                            updateSetting("dateFormat", value);
                          }}
                        />
                        <SettingsSelect
                          label={t("settings.format.time")}
                          value={timeFormat}
                          options={TIME_FORMAT_OPTIONS}
                          onChange={(value) => {
                            updateSetting("timeFormat", value);
                          }}
                        />
                      </div>
                    ),
                  },
                ]}
              />

              <SettingsCategory
                title={t("settings.categories.appearance")}
                sections={[
                  {
                    title: t("settings.theme.sourceTitle"),
                    description: t("settings.theme.sourceDescription"),
                    body: (
                      <div className="grid grid-cols-2 gap-2">
                        {UI_THEME_MODES.map((m) => (
                          <ItemSelect
                            key={m.id}
                            selected={uiThemeMode === m.id}
                            title={t(`settings.theme.mode.${m.id}`)}
                            description={t(`settings.theme.mode.${m.id}Description`)}
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
                          title: t("settings.theme.title"),
                          description: t("settings.theme.titleDescription"),
                          body: (
                            <div className="grid grid-cols-2 gap-2">
                              {UI_THEMES.map((th) => (
                                <ItemSelect
                                  key={th.id}
                                  selected={uiThemeManual === th.id}
                                  title={t(`settings.theme.${th.id}`)}
                                  onSelect={() => {
                                    updateSetting("uiThemeManual", th.id);
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
                title={t("settings.categories.map")}
                sections={[
                  {
                    title: t("settings.map.styleSource.title"),
                    description: t("settings.map.styleSource.description"),
                    body: (
                      <div className="grid grid-cols-3 gap-2">
                        {MAP_THEME_MODES.map((m) => (
                          <ItemSelect
                            key={m.id}
                            selected={mapThemeMode === m.id}
                            title={t(`settings.theme.mode.${m.id}`)}
                            description={t(`settings.theme.mode.${m.id}Description`)}
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
                          title: t("settings.map.style.title"),
                          description: t("settings.map.style.description"),
                          body: (
                            <div className="grid grid-cols-2 gap-2">
                              {MAP_STYLES.map((s) => (
                                <ItemSelect
                                  key={s.id}
                                  selected={currentStyle === s.id}
                                  title={t(`settings.map.style.${s.id}`)}
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
                    title: t("settings.map.visibility.title"),
                    description: t("settings.map.visibility.description"),
                    body: (
                      <div className="grid grid-cols-2 gap-2">
                        <OnOffSwitch
                          enabled={showBuses}
                          title={t("settings.map.vehicle.bus")}
                          onToggle={() => {
                            updateSetting("showBuses", !showBuses);
                          }}
                        />
                        <OnOffSwitch
                          enabled={showTrams}
                          title={t("settings.map.vehicle.tram")}
                          onToggle={() => {
                            updateSetting("showTrams", !showTrams);
                          }}
                        />
                        <OnOffSwitch
                          enabled={showGbfsStations}
                          title={t("settings.map.vehicle.bike")}
                          onToggle={() => {
                            updateSetting("showGbfsStations", !showGbfsStations);
                          }}
                        />
                      </div>
                    ),
                  },
                  {
                    title: t("settings.map.motion.title"),
                    description: t("settings.map.motion.description"),
                    body: (
                      <div className="grid grid-cols-2 gap-2">
                        {VEHICLE_MOTIONS.map((m) => (
                          <ItemSelect
                            key={m.id}
                            selected={vehicleMotion === m.id}
                            title={t(`settings.map.motion.${m.id}`)}
                            description={t(`settings.map.motion.${m.id}Description`)}
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
                title={t("settings.categories.general")}
                sections={[
                  {
                    title: t("settings.general.awake.title"),
                    description: t("settings.general.awake.description"),
                    body: (
                      <OnOffSwitch
                        enabled={wakeLockEnabled}
                        title={
                          wakeLockEnabled
                            ? t("settings.general.awake.on")
                            : t("settings.general.awake.off")
                        }
                        onToggle={() => {
                          updateSetting("wakeLockEnabled", !wakeLockEnabled);
                        }}
                      />
                    ),
                  },
                  {
                    title: t("settings.general.feedback.title"),
                    description: t("settings.general.feedback.description"),
                    body: (
                      <button
                        type="button"
                        onClick={() => {
                          settingsStore.setState({ _settingsOpen: false });
                          openFeedback();
                        }}
                        className="border-outline bg-surface text-on-surface-variant hover:bg-surface-hover flex w-full cursor-pointer items-center justify-between rounded-lg border px-3 py-2.5 text-sm font-medium transition-colors"
                      >
                        <span>{t("settings.general.feedback.action")}</span>
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
  const t = useT();
  const open = settingsStore((s) => s._settingsOpen);

  return (
    <button
      type="button"
      aria-label={t("settings.aria.open")}
      onClick={() => {
        settingsStore.setState({ _settingsOpen: true });
      }}
      aria-expanded={open}
      aria-controls="settings-panel"
      className="bg-surface-overlay text-on-surface-variant hover:bg-surface flex h-9 w-9 cursor-pointer items-center justify-center rounded-lg shadow-md backdrop-blur-sm transition-colors"
      title={t("settings.title")}
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
  const t = useT();
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
        <span>
          {props.title ??
            (props.enabled ? t("settings.state.visible") : t("settings.state.hidden"))}
        </span>
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
        {props.enabled ? t("settings.state.on") : t("settings.state.off")}
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

function SettingsSelect<T extends string>(props: {
  label: string;
  value: T;
  options: readonly { id: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <label className="text-on-surface-muted flex flex-col gap-1 text-xs">
      {props.label}
      <select
        value={props.value}
        onChange={(e) => {
          const option = props.options.find((o) => o.id === e.target.value);
          if (option) props.onChange(option.id);
        }}
        className="border-outline bg-surface text-on-surface hover:bg-surface-hover cursor-pointer rounded-lg border px-3 py-2 text-sm font-medium tabular-nums transition-colors"
      >
        {props.options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
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
