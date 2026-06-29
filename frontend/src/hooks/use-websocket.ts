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

    async function connectWebSocket() {
      if (signal.aborted) return null;

      const url = new URL(`${API_URL}/v1/ws`, window.location.href);
      url.protocol = url.protocol === "https:" ? "wss:" : "ws:";

      console.log("Connecting to WebSocket", url.pathname);
      const ws = new WebSocket(url.toString());
      wsRef.current = ws;

      return new Promise<null>((resolve) => {
        const onAbort = () => {
          ws.close();
        };
        signal.addEventListener("abort", onAbort, { once: true });

        ws.addEventListener(
          "open",
          (e) => {
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
            console.error("WebSocket error", e);
            useStore.setState({ lastError: "Connection error", wsConnected: false });
            ws.close();
          },
          { signal },
        );

        ws.addEventListener(
          "close",
          (e) => {
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

    async function loop() {
      while (!signal.aborted) {
        await connectWebSocket();
        if (signal.aborted) break;
        const sleepFor = 3000 + 10_000 * Math.random();
        console.log("Sleeping before reconnect", sleepFor);
        await new Promise((resolve) => {
          const timer = setTimeout(resolve, sleepFor);
          signal.addEventListener(
            "abort",
            () => {
              clearTimeout(timer);
            },
            { once: true },
          );
        });
      }
    }

    void loop();

    return () => {
      abortController.abort();
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
