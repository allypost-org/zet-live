import { useEffect, useState } from "react";
import { renderText, useT, type TFn } from "@/i18n";
import { useStore } from "@/store";

function formatAgo(ms: number, t: TFn): string {
  const seconds = Math.floor(ms / 1000);
  if (seconds < 1) return t("status.justNow");
  if (seconds < 60) return t("status.secondsAgo", { count: seconds });
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t("status.minutesAgo", { count: minutes });
  return t("status.hoursAgo", { count: Math.floor(minutes / 60) });
}

export function StatusBar() {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const lastUpdate = useStore((s) => s.lastUpdate);
  const lastError = useStore((s) => s.lastError);
  const wsConnected = useStore((s) => s.wsConnected);
  const [, setTick] = useState(0);

  useEffect(() => {
    const id = setInterval(() => {
      setTick((t) => t + 1);
    }, 1000);
    return () => {
      clearInterval(id);
    };
  }, []);

  const ago = lastUpdate ? formatAgo(Date.now() - lastUpdate, t) : t("status.never");

  return (
    <button
      type="button"
      aria-label={t("status.aria.label")}
      aria-expanded={expanded}
      aria-controls="connection-status-panel"
      aria-haspopup="menu"
      aria-roledescription={t("status.aria.label")}
      role="button"
      onClick={() => {
        setExpanded((e) => !e);
      }}
      className="bg-surface-overlay text-on-surface flex cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-xs shadow-md backdrop-blur-sm select-none"
    >
      <span
        className={`inline-block h-2 w-2 shrink-0 rounded-full ${wsConnected ? "bg-success" : "bg-danger animate-pulse"}`}
      />
      {expanded && (
        <div
          aria-label={t("status.aria.details")}
          aria-modal="true"
          role="menu"
          id="connection-status-panel"
          className="flex items-center gap-2"
        >
          <span>{wsConnected ? t("status.connected") : t("status.disconnected")}</span>
          <span className="text-on-surface-muted">|</span>
          <span>{t("status.lastUpdate", { ago })}</span>
          {lastError && (
            <>
              <span className="text-on-surface-muted">|</span>
              <span className="text-on-danger-container">{renderText(lastError, t)}</span>
            </>
          )}
        </div>
      )}
    </button>
  );
}
