import { useEffect, useCallback, useRef } from "react";
import {
  getSharedWorker,
  postWorkerMessage,
  startWorkerStopsFetching,
  stopWorkerStopsFetching,
  type WorkerResponse,
} from "./use-worker";
import { API_URL } from "@/app/consts";
import { processMessage, handleStopsUpdate } from "./use-stops";
import { useStore } from "@/store";
import { toast } from "sonner";
import { authStore, sessionToken, clearAuth } from "@/auth-store";
import { setFeatureFlags } from "@/feature-flags-store";

const CONNECT_TIMEOUT_MS = 10_000;
const RECONNECT_DELAY_MIN_MS = 3_000;
const RECONNECT_DELAY_SPREAD_MS = 10_000;

function sendAuthMessage(ws: WebSocket, token: string | null) {
  ws.send(JSON.stringify({ v: 1, t: "auth", d: token }));
}

export function useWebSocket() {
  const token = authStore((s) => s.token);
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    const worker = getSharedWorker();

    const handler = (e: MessageEvent<WorkerResponse>) => {
      const response = e.data;
      switch (response.type) {
        case "processed-message": {
          const data = response.data.d;
          if (typeof data === "object" && "notices" in data) {
            useStore.setState({ globalNotices: data.notices.length > 0 ? data.notices : null });
            return;
          }
          if (typeof data === "object" && "userNotices" in data) {
            useStore.setState({
              userNotices: data.userNotices.length > 0 ? data.userNotices : null,
            });
            return;
          }
          if (typeof data === "object" && "toast" in data) {
            const { message, type: toastType, duration } = data.toast;
            toast[toastType](message, { duration });
            return;
          }
          if (typeof data === "object" && "featureFlags" in data) {
            setFeatureFlags(data.featureFlags);
            return;
          }
          processMessage(response.data);
          return;
        }
        case "stops-update": {
          handleStopsUpdate(response);
          return;
        }
        default: {
          console.error("Unknown message type from worker", (response as { type: string }).type);
          return;
        }
      }
    };

    worker.addEventListener("message", handler);

    startWorkerStopsFetching(worker);

    return () => {
      stopWorkerStopsFetching(worker);
      worker.removeEventListener("message", handler);
    };
  }, []);

  const sendToWorker = useCallback((data: Blob) => {
    const worker = getSharedWorker();
    postWorkerMessage(worker, data);
    useStore.setState({ lastUpdate: Date.now(), lastError: null });
  }, []);

  useEffect(() => {
    const abortController = new AbortController();
    const { signal } = abortController;

    let wakeSleep: (() => void) | null = null;
    let resumeRequested = false;

    async function connectWebSocket() {
      if (signal.aborted) return null;

      const url = new URL(`${API_URL}/v1/ws`, window.location.href);
      url.protocol = url.protocol === "https:" ? "wss:" : "ws:";

      console.log("Connecting to WebSocket", url.pathname);
      const ws = new WebSocket(url.toString());
      wsRef.current = ws;

      return new Promise<null>((resolve) => {
        // A suspended mobile tab can lose its radio mid-handshake, leaving the socket
        // stuck in CONNECTING with no open, error, or close event. Nothing below would
        // ever resolve, so the reconnect loop would stop for good.
        const connectTimer = setTimeout(() => {
          useStore.setState({ lastError: "Connection timed out", wsConnected: false });
          ws.close();
        }, CONNECT_TIMEOUT_MS);

        const onAbort = () => {
          clearTimeout(connectTimer);
          ws.close();
        };

        signal.addEventListener("abort", onAbort, { once: true });

        ws.addEventListener(
          "open",
          (e) => {
            clearTimeout(connectTimer);
            console.log("WebSocket opened", e);
            useStore.setState({ wsConnected: true, lastError: null });
            const tok = sessionToken();
            if (tok) sendAuthMessage(ws, tok);
          },
          { signal },
        );

        ws.addEventListener(
          "error",
          (e) => {
            clearTimeout(connectTimer);
            console.error("WebSocket error", e);
            useStore.setState({ lastError: "Connection error", wsConnected: false });
            ws.close();
          },
          { signal },
        );

        ws.addEventListener(
          "close",
          (e) => {
            clearTimeout(connectTimer);
            console.log("WebSocket closed", e);
            useStore.setState({ wsConnected: false });
            signal.removeEventListener("abort", onAbort);
            resolve(null);
          },
          { once: true },
        );

        ws.addEventListener(
          "message",
          (e) => {
            if (typeof e.data === "string") {
              try {
                const msg = JSON.parse(e.data) as { d?: unknown };
                if (msg.d && typeof msg.d === "object" && "sessionRevoked" in msg.d) {
                  clearAuth();
                  toast.info("Your session has been revoked");
                }
              } catch {
                /* ignore malformed text */
              }
            } else {
              console.log("Got data", { len: (e.data as Blob).size });
              sendToWorker(e.data as Blob);
            }
          },
          { signal },
        );
      });
    }

    // The socket usually dies while the page is frozen in the background, so by the
    // time the user comes back the reconnect delay below is counting down against a
    // radio that is working again. Skip the remainder of it and reconnect now.
    function reconnectNow() {
      resumeRequested = true;
      const ws = wsRef.current;
      if (ws && ws.readyState !== WebSocket.OPEN) {
        ws.close();
      }
      wakeSleep?.();
    }

    function sleepUntilWoken() {
      const sleepFor = RECONNECT_DELAY_MIN_MS + RECONNECT_DELAY_SPREAD_MS * Math.random();
      console.log("Sleeping before reconnect", sleepFor);
      return new Promise<void>((resolve) => {
        const finish = () => {
          clearTimeout(timer);
          signal.removeEventListener("abort", finish);
          wakeSleep = null;
          resolve();
        };
        const timer = setTimeout(finish, sleepFor);
        wakeSleep = finish;
        signal.addEventListener("abort", finish, { once: true });
      });
    }

    async function loop() {
      while (!signal.aborted) {
        resumeRequested = false;
        await connectWebSocket();
        if (signal.aborted) break;
        if (resumeRequested) continue;
        await sleepUntilWoken();
      }
    }

    function onVisibilityChange() {
      if (document.visibilityState === "visible") reconnectNow();
    }

    // Only on a back/forward-cache restore. A plain pageshow also fires on first load,
    // which would abort the very first handshake for nothing.
    function onPageShow(e: PageTransitionEvent) {
      if (e.persisted) reconnectNow();
    }

    document.addEventListener("visibilitychange", onVisibilityChange);
    window.addEventListener("online", reconnectNow);
    window.addEventListener("focus", reconnectNow);
    window.addEventListener("pageshow", onPageShow);

    void loop();

    return () => {
      abortController.abort();
      document.removeEventListener("visibilitychange", onVisibilityChange);
      window.removeEventListener("online", reconnectNow);
      window.removeEventListener("focus", reconnectNow);
      window.removeEventListener("pageshow", onPageShow);
      wsRef.current = null;
    };
  }, [sendToWorker]);

  useEffect(() => {
    const ws = wsRef.current;
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    sendAuthMessage(ws, token);
  }, [token]);

  return { sendToWorker };
}
