import { useStore } from "@/store";
import type { GlobalNotice } from "@/app/entity/v1/message";
import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";

const DISMISSED_KEY = "dismissed-notice-ids";

function getDismissedIdsRaw() {
  try {
    const raw = window.localStorage.getItem(DISMISSED_KEY);
    if (!raw) return [];
    return JSON.parse(raw) as { id: string; dismissedAt: number }[];
  } catch {
    return [];
  }
}

function getDismissedIds() {
  return new Set(getDismissedIdsRaw().map((x) => x.id));
}

const DISMISS_EXPIRATION_TIME = 30 * 24 * 60 * 60 * 1000;
function cleanupDismissedIds() {
  const ids = getDismissedIdsRaw();
  const filtered = ids.filter((x) => x.dismissedAt > Date.now() - DISMISS_EXPIRATION_TIME);
  if (filtered.length !== ids.length) {
    setDismissedIds(filtered);
    return true;
  }

  return false;
}

function setDismissedIds(ids: { id: string; dismissedAt: number }[]) {
  window.localStorage.setItem(DISMISSED_KEY, JSON.stringify(ids));
}

function dismissId(id: string) {
  const dismissedIds = getDismissedIdsRaw();
  dismissedIds.push({ id, dismissedAt: Date.now() });
  setDismissedIds(dismissedIds);
}

type Severity = "info" | "warning" | "error";

const severityStyles: Record<Severity, string> = {
  info: "bg-blue-500/90 text-white",
  warning: "bg-amber-500/90 text-white",
  error: "bg-red-500/90 text-white",
};

const severityIcons: Record<Severity, string> = {
  info: "i",
  warning: "!",
  error: "x",
};

function getStyle(severity: Severity): string {
  return severityStyles[severity] ?? severityStyles.info;
}

function getIcon(severity: Severity): string {
  return severityIcons[severity] ?? severityIcons.info;
}

function NoticeItem({ notice, onDismiss }: { notice: GlobalNotice; onDismiss: () => void }) {
  const style = getStyle(notice.severity);
  const icon = getIcon(notice.severity);

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: 10 }}
      transition={{ duration: 0.15 }}
      className={`pointer-events-auto flex items-center gap-2 rounded-lg px-3 py-2 text-sm shadow-lg ${style}`}
    >
      <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-white/20 text-xs font-bold">
        {icon}
      </span>
      <span className="flex-1">{notice.text}</span>
      <button
        type="button"
        onClick={onDismiss}
        className="ml-2 shrink-0 rounded p-0.5 opacity-70 transition-opacity hover:opacity-100"
        aria-label="Dismiss notice"
      >
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path
            d="M2 2L12 12M12 2L2 12"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
          />
        </svg>
      </button>
    </motion.div>
  );
}

export function NoticeBar() {
  const globalNotices = useStore((s) => s.globalNotices);
  const userNotices = useStore((s) => s.userNotices);
  const [dismissedIds, setDismissedState] = useState<Set<string>>(getDismissedIds);

  useEffect(() => {
    const cleanup = () => {
      if (cleanupDismissedIds()) {
        setDismissedState(getDismissedIds());
      }
    };

    cleanup();
    const interval = setInterval(cleanup, 60_000);
    return () => {
      clearInterval(interval);
    };
  }, []);

  useEffect(() => {
    const handler = (e: StorageEvent) => {
      if (e.key === DISMISSED_KEY) {
        setDismissedState(getDismissedIds());
      }
    };
    window.addEventListener("storage", handler);
    return () => {
      window.removeEventListener("storage", handler);
    };
  }, []);

  const allNotices = [...(globalNotices ?? []), ...(userNotices ?? [])];
  if (allNotices.length === 0) return null;

  const visible = allNotices.filter((n) => !dismissedIds.has(n.id));
  if (visible.length === 0) return null;

  const dismissNotice = (id: string) => {
    dismissId(id);
    setDismissedState(getDismissedIds());
  };

  return (
    <div className="pointer-events-none flex flex-col gap-1.5">
      <AnimatePresence>
        {visible.map((notice) => (
          <NoticeItem
            key={notice.id}
            notice={notice}
            onDismiss={() => {
              dismissNotice(notice.id);
            }}
          />
        ))}
      </AnimatePresence>
    </div>
  );
}
