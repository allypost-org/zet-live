import { useEffect, useRef, useState } from "react";
import { useStore } from "@/store";

type Step = {
  label: string;
  done: boolean;
  optional?: boolean;
};

const FADE_DURATION = 250;
const MIN_LOADING_TIME = 150;

function CheckIcon({ done }: { done: boolean }) {
  if (!done) {
    return (
      <span className="border-on-surface-faint inline-block h-4 w-4 shrink-0 rounded-full border-2" />
    );
  }

  return (
    <span className="bg-success inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full">
      <svg
        className="text-on-primary h-3 w-3"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <polyline points="20 6 9 17 4 12" />
      </svg>
    </span>
  );
}

export function LoadingScreen() {
  const wsConnected = useStore((s) => s.wsConnected);
  const lastUpdate = useStore((s) => s.lastUpdate);
  const simpleStops = useStore((s) => s.simpleStops);
  const mapReady = useStore((s) => s.mapReady);

  const mountTime = useRef(Date.now());
  const [fading, setFading] = useState(false);
  const [hidden, setHidden] = useState(false);

  const vehiclesLoaded = lastUpdate !== null;
  const stopsLoaded = Object.keys(simpleStops).length > 0;

  const steps: Step[] = [
    { label: "Connecting to API", done: wsConnected },
    { label: "Loading vehicles", done: vehiclesLoaded },
    { label: "Loading stops", done: stopsLoaded, optional: true },
    { label: "Loading map", done: mapReady },
  ];

  const allDone = steps.every((s) => s.done || s.optional);

  useEffect(() => {
    if (!allDone) return;

    const elapsed = Date.now() - mountTime.current;
    const remaining = Math.max(0, MIN_LOADING_TIME - elapsed);

    if (remaining > 0) {
      const hideTimer = setTimeout(() => {
        setHidden(true);
      }, remaining);
      return () => {
        clearTimeout(hideTimer);
      };
    }

    const fadeTimer = setTimeout(() => {
      setFading(true);
    }, 0);
    const hideTimer = setTimeout(() => {
      setHidden(true);
    }, FADE_DURATION);

    return () => {
      clearTimeout(fadeTimer);
      clearTimeout(hideTimer);
    };
  }, [allDone]);

  if (hidden) return null;

  return (
    <div
      className="bg-surface fixed inset-0 z-9999 flex items-center justify-center"
      style={fading ? { transition: `opacity ${FADE_DURATION}ms ease-out`, opacity: 0 } : undefined}
    >
      <div className="flex flex-col items-center gap-6">
        <div className="flex flex-col items-center gap-2">
          <h1 className="text-on-surface text-2xl font-bold">ZET Live</h1>
          <div className="border-outline border-t-on-surface h-5 w-5 animate-spin rounded-full border-2" />
        </div>

        <ul className="flex flex-col gap-2">
          {steps.map((step) => (
            <li
              key={step.label}
              className="text-on-surface-variant flex items-center gap-2 text-sm"
            >
              <CheckIcon done={step.done} />
              <span className={step.done ? "text-on-surface-faint line-through" : ""}>
                {step.label}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
