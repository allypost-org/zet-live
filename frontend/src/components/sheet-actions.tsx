import type { ReactNode } from "react";
import { toast } from "sonner";

const ICON_PROPS = {
  xmlns: "http://www.w3.org/2000/svg",
  width: 14,
  height: 14,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

export function ShareIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8" />
      <polyline points="16 6 12 2 8 6" />
      <line x1="12" y1="2" x2="12" y2="15" />
    </svg>
  );
}

export function CenterIcon() {
  return (
    <svg {...ICON_PROPS}>
      <circle cx="12" cy="12" r="10" />
      <polygon points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76" />
    </svg>
  );
}

export function FitAllIcon() {
  return (
    <svg {...ICON_PROPS}>
      <polyline points="15 3 21 3 21 9" />
      <polyline points="9 21 3 21 3 15" />
      <line x1="21" y1="3" x2="14" y2="10" />
      <line x1="3" y1="21" x2="10" y2="14" />
    </svg>
  );
}

const ACTION_BUTTON_CLASS =
  "bg-surface-dim text-on-surface-muted hover:bg-surface-hover flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-semibold transition-colors disabled:pointer-events-none disabled:opacity-40";

export type SheetAction = {
  key: string;
  label: string;
  icon: ReactNode;
  onClick: () => void;
  disabled?: boolean;
};

/** Pinned footer of the bottom sheet. `leading` sits left, actions are right-aligned. */
export function SheetActions({
  leading,
  actions,
}: {
  leading?: ReactNode;
  actions: SheetAction[];
}) {
  return (
    <div className="border-outline flex shrink-0 items-center gap-2 border-t px-4 py-2">
      {leading}
      <div className="ml-auto flex items-center gap-1.5">
        {actions.map((action) => (
          <button
            key={action.key}
            type="button"
            onClick={action.onClick}
            disabled={action.disabled ?? false}
            className={ACTION_BUTTON_CLASS}
          >
            {action.icon}
            {action.label}
          </button>
        ))}
      </div>
    </div>
  );
}

/** Share sheet link, falling back to the clipboard where the Web Share API is absent. */
export function shareSheetLink(title: string, params: URLSearchParams) {
  const url = `${location.origin}${location.pathname}?${params}`;
  if (navigator.share) {
    navigator.share({ title, url }).catch(() => {});
    return;
  }
  navigator.clipboard.writeText(url).then(
    () =>
      toast.success("Link copied to clipboard", {
        dismissible: true,
      }),
    () => {},
  );
}
