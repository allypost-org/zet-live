import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { animate, motion, useMotionValue } from "motion/react";
import {
  appRequestAnimationFrame,
  cancelAnimationOrIdleCallback,
} from "@/utils/polyfill/requestSomeCallback";

type SheetState = "minimized" | "expanded" | "maximized";

type Props = {
  open: boolean;
  title: ReactNode;
  onClose: () => void;
  children: ReactNode;
  minimizedBody?: ReactNode;
  expandedHeight?: string;
  maximizedHeight?: string;
};

type DragInfo = {
  offset: { x: number; y: number };
  velocity: { x: number; y: number };
};

const SPRING = { type: "spring" as const, damping: 25, stiffness: 300 };
const ENTER = { type: "tween" as const, duration: 0.3, ease: [0.32, 0.72, 0, 1] as const };
const DRAG_THRESHOLD = 50;
const VELOCITY_THRESHOLD = 500;

export function BottomSheet({
  open,
  title,
  onClose,
  children,
  minimizedBody,
  expandedHeight = "40dvh",
  maximizedHeight = "calc(95dvh - 36px)",
}: Props) {
  const [sheetState, setSheetState] = useState<SheetState>("expanded");
  const [rendered, setRendered] = useState(false);
  const sheetRef = useRef<HTMLDivElement>(null);
  const y = useMotionValue(0);
  const height = useMotionValue(0);
  const headerRef = useRef<HTMLDivElement>(null);
  const summaryRef = useRef<HTMLDivElement>(null);
  const expandedSizeRef = useRef<HTMLDivElement>(null);
  const maximizedSizeRef = useRef<HTMLDivElement>(null);
  const dragStartHeight = useRef(0);
  const dragging = useRef(false);
  const suppressClick = useRef(false);
  const contentRef = useRef<HTMLDivElement>(null);
  const contentHeightRef = useRef(0);

  const minimized = sheetState === "minimized";
  const maximized = sheetState === "maximized";

  const measureContent = useCallback(() => {
    const body = contentRef.current?.firstElementChild as HTMLElement | null;
    if (!body) return 0;
    const previous = body.style.maxHeight;
    body.style.maxHeight = "none";
    const measured = body.offsetHeight;
    body.style.maxHeight = previous;
    return measured;
  }, []);

  const getHeights = useCallback(() => {
    const headerHeight = headerRef.current?.offsetHeight ?? 0;
    const minimizedHeight = headerHeight + (summaryRef.current?.offsetHeight ?? 0);
    const contentWithHeader = headerHeight + contentHeightRef.current;
    const expandedCap = expandedSizeRef.current?.offsetHeight ?? Infinity;
    const maximizedCap = maximizedSizeRef.current?.offsetHeight ?? Infinity;

    const expanded = Math.max(minimizedHeight, Math.min(contentWithHeader, expandedCap));
    return {
      minimized: minimizedHeight,
      expanded,
      maximized: Math.max(expanded, Math.min(contentWithHeader, maximizedCap)),
    };
  }, []);

  const getDismissY = () => (sheetRef.current?.offsetHeight ?? 400) + 20;

  useLayoutEffect(() => {
    if (!rendered) return;
    const region = contentRef.current;
    if (!region) return;

    const resize = () => {
      if (dragging.current) return;
      contentHeightRef.current = measureContent();
      const target = getHeights()[sheetState];
      if (height.get() === 0) height.set(target);
      else animate(height, target, SPRING);
    };
    resize();

    let bodyObserver: ResizeObserver | null = null;
    const observeBody = () => {
      bodyObserver?.disconnect();
      bodyObserver = new ResizeObserver(resize);
      const body = region.firstElementChild;
      if (body) bodyObserver.observe(body);
    };
    observeBody();

    const swapObserver = new MutationObserver(() => {
      observeBody();
      resize();
    });
    swapObserver.observe(region, { childList: true });

    const chromeObserver = new ResizeObserver(resize);
    for (const element of [headerRef.current, summaryRef.current]) {
      if (element) chromeObserver.observe(element);
    }

    return () => {
      bodyObserver?.disconnect();
      swapObserver.disconnect();
      chromeObserver.disconnect();
    };
  }, [rendered, sheetState, getHeights, measureContent, height]);

  useEffect(() => {
    if (open) {
      dragging.current = false;
      setSheetState("expanded");
      y.set(2000);
      setRendered(true);
    }
  }, [open, y]);

  useEffect(() => {
    if (!rendered) return;

    let timeout: number | null = null;
    if (open) {
      const sheetHeight = sheetRef.current?.offsetHeight ?? 300;
      y.set(sheetHeight);
      timeout = appRequestAnimationFrame(() => {
        animate(y, 0, ENTER);
      });
    } else {
      const sheetHeight = sheetRef.current?.offsetHeight ?? 300;
      animate(y, sheetHeight, {
        ...SPRING,
        onComplete: () => {
          setRendered(false);
        },
      });
    }

    return () => {
      cancelAnimationOrIdleCallback(timeout);
    };
  }, [rendered, open, y]);

  if (!rendered) return null;

  return (
    <div className="pointer-events-none fixed right-0 bottom-0 left-0 z-999 flex justify-center">
      <motion.div
        aria-label="Bottom sheet"
        aria-modal="true"
        role="dialog"
        ref={sheetRef}
        style={{ y, height }}
        className="bg-surface-overlay pointer-events-auto relative flex w-full max-w-md flex-col overflow-hidden rounded-t-xl shadow-lg backdrop-blur-sm"
        data-minimized={minimized ? "true" : "false"}
        data-maximized={maximized ? "true" : "false"}
      >
        <motion.div
          ref={headerRef}
          onPointerDown={() => {
            suppressClick.current = false;
          }}
          onClickCapture={(event) => {
            if (suppressClick.current && event.detail !== 0) {
              event.preventDefault();
              event.stopPropagation();
            }
          }}
          onPanStart={() => {
            dragging.current = true;
            suppressClick.current = true;
            height.stop();
            y.stop();
            y.set(0);
            dragStartHeight.current = height.get();
          }}
          onPan={(_, info: DragInfo) => {
            const heights = getHeights();
            height.set(
              Math.max(
                heights.minimized,
                Math.min(heights.maximized, dragStartHeight.current - info.offset.y),
              ),
            );
          }}
          onPanEnd={(event, info: DragInfo) => {
            dragging.current = false;
            if (event.type === "pointercancel") {
              animate(height, getHeights()[sheetState], SPRING);
              return;
            }
            const down = info.offset.y > DRAG_THRESHOLD || info.velocity.y > VELOCITY_THRESHOLD;
            const up = info.offset.y < -DRAG_THRESHOLD || info.velocity.y < -VELOCITY_THRESHOLD;
            if (sheetState === "minimized" && down) {
              animate(y, getDismissY(), { ...SPRING, onComplete: onClose });
              return;
            }
            let next = sheetState;
            if (down) next = maximized ? "expanded" : "minimized";
            else if (up) next = minimized ? "expanded" : "maximized";
            setSheetState(next);
            animate(height, getHeights()[next], SPRING);
          }}
          className="relative flex shrink-0 cursor-grab touch-none items-center justify-between gap-2 p-4 pb-0 select-none active:cursor-grabbing max-sm:pb-0"
        >
          <div
            aria-hidden="true"
            className="bg-on-surface-faint/40 absolute top-2 left-1/2 h-1 w-10 -translate-x-1/2 rounded-full"
          />
          <div
            className="min-w-0 flex-1 select-none"
            onClick={() => {
              setSheetState((s) => (s === "minimized" ? "expanded" : "minimized"));
            }}
          >
            {title}
          </div>
          <div
            className="flex shrink-0 items-center gap-1"
            onPointerDownCapture={(event) => {
              event.stopPropagation();
              suppressClick.current = false;
            }}
          >
            <button
              type="button"
              aria-label={minimized ? "Expand sheet" : "Minimize sheet"}
              onClick={() => {
                setSheetState((s) => (s === "minimized" ? "expanded" : "minimized"));
              }}
              className="text-on-surface-faint hover:bg-surface-hover hover:text-on-surface-muted flex size-11 items-center justify-center rounded-full transition-colors"
            >
              {minimized ? (
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
                  <polyline points="18 15 12 9 6 15" />
                </svg>
              ) : (
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
                  <polyline points="6 9 12 15 18 9" />
                </svg>
              )}
            </button>
            <button
              type="button"
              aria-label="Close sheet"
              onClick={onClose}
              className="text-on-surface-faint hover:bg-surface-hover hover:text-on-surface-muted flex size-11 items-center justify-center rounded-full transition-colors"
            >
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
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>
        </motion.div>

        <div
          ref={contentRef}
          aria-hidden={minimized}
          inert={minimized}
          className="min-h-0 flex-1 overflow-auto overscroll-y-contain"
        >
          {children}
        </div>

        {minimizedBody && (
          <div
            ref={summaryRef}
            aria-hidden={!minimized}
            className={
              minimized
                ? "shrink-0 px-4 pb-3"
                : "invisible absolute right-0 bottom-0 left-0 px-4 pb-3"
            }
          >
            {minimizedBody}
          </div>
        )}
      </motion.div>
      <div aria-hidden="true" className="pointer-events-none invisible fixed w-0 overflow-hidden">
        <div ref={expandedSizeRef} style={{ height: expandedHeight }} />
        <div ref={maximizedSizeRef} style={{ height: maximizedHeight }} />
      </div>
    </div>
  );
}
