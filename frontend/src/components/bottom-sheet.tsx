import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { animate, motion, useDragControls, useMotionValue } from "motion/react";
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
  maximizedHeight = "95dvh",
}: Props) {
  const [sheetState, setSheetState] = useState<SheetState>("expanded");
  const [rendered, setRendered] = useState(false);
  const sheetRef = useRef<HTMLDivElement>(null);
  const y = useMotionValue(0);
  const dragControls = useDragControls();

  const minimized = sheetState === "minimized";
  const maximized = sheetState === "maximized";

  const getDismissY = () => (sheetRef.current?.offsetHeight ?? 400) + 20;

  useEffect(() => {
    if (open) {
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
        style={{ y }}
        animate={{
          maxHeight: maximized ? maximizedHeight : expandedHeight,
        }}
        initial={false}
        drag="y"
        dragControls={dragControls}
        dragListener={false}
        dragConstraints={{ top: -150, bottom: getDismissY() }}
        dragElastic={0.1}
        dragMomentum={false}
        onDragEnd={(_: Event, info: DragInfo) => {
          const downPastThreshold =
            info.offset.y > DRAG_THRESHOLD || info.velocity.y > VELOCITY_THRESHOLD;
          const upPastThreshold =
            info.offset.y < -DRAG_THRESHOLD || info.velocity.y < -VELOCITY_THRESHOLD;

          if (sheetState === "minimized") {
            if (downPastThreshold) {
              animate(y, getDismissY(), { ...SPRING, onComplete: onClose });
            } else if (upPastThreshold) {
              animate(y, 0, SPRING);
              setSheetState("expanded");
            } else {
              animate(y, 0, SPRING);
            }
          } else if (sheetState === "expanded") {
            if (downPastThreshold) {
              animate(y, 0, SPRING);
              setSheetState("minimized");
            } else if (upPastThreshold) {
              animate(y, 0, SPRING);
              setSheetState("maximized");
            } else {
              animate(y, 0, SPRING);
            }
          } else {
            if (downPastThreshold) {
              animate(y, 0, SPRING);
              setSheetState("expanded");
            } else {
              animate(y, 0, SPRING);
            }
          }
        }}
        className="bg-surface-overlay pointer-events-auto grid w-full max-w-md grid-rows-[auto_1fr_auto] overflow-hidden rounded-t-xl shadow-lg backdrop-blur-sm"
        data-minimized={minimized ? "true" : "false"}
        data-maximized={maximized ? "true" : "false"}
      >
        <div
          onPointerDown={(e) => {
            dragControls.start(e);
          }}
          className="flex shrink-0 cursor-grab items-center justify-between gap-2 px-4 py-3 active:cursor-grabbing"
        >
          <div
            className="min-w-0 flex-1 select-none"
            onClick={() => {
              setSheetState((s) => (s === "minimized" ? "expanded" : "minimized"));
            }}
          >
            {title}
          </div>
          <div className="flex shrink-0 items-center gap-1">
            <button
              type="button"
              onClick={() => {
                setSheetState((s) => (s === "minimized" ? "expanded" : "minimized"));
              }}
              className="text-on-surface-faint hover:bg-surface-hover hover:text-on-surface-muted rounded-full p-1 transition-colors"
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
              onClick={onClose}
              className="text-on-surface-faint hover:bg-surface-hover hover:text-on-surface-muted rounded-full p-1 transition-colors"
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
        </div>

        <motion.div
          animate={{
            height: minimized ? 0 : "auto",
            opacity: minimized ? 0 : 1,
          }}
          transition={SPRING}
          className="max-h-full overflow-auto overscroll-y-contain"
        >
          {children}
        </motion.div>

        {minimizedBody && (
          <motion.div
            animate={{
              height: minimized ? "auto" : 0,
              opacity: minimized ? 1 : 0,
            }}
            transition={SPRING}
            className="overflow-hidden"
          >
            <div className="px-4 pb-3">{minimizedBody}</div>
          </motion.div>
        )}
      </motion.div>
    </div>
  );
}
