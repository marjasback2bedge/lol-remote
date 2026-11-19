import { useEffect, useRef, useState } from "preact/hooks";
import { api, ApiError } from "./api";
import type { State } from "./types";

export type Live = {
  state: State | null;
  /** WebSocket currently open. */
  online: boolean;
  /** Token rejected: the user needs to re-scan the QR code. */
  unauthorized: boolean;
  /** Local `Date.now()` when `state` arrived. */
  receivedLocal: number;
};

const MAX_BACKOFF = 5000;

/** Subscribes to `/ws`, reconnecting with exponential backoff (spec §9.2). */
export function useLiveState(): Live {
  const [live, setLive] = useState<Live>({
    state: null,
    online: false,
    unauthorized: false,
    receivedLocal: 0,
  });

  useEffect(() => {
    let ws: WebSocket | null = null;
    let attempt = 0;
    let timer: number | undefined;
    let stopped = false;

    const connect = async () => {
      // A failed WebSocket upgrade hides the status code, so check auth first.
      try {
        await api("GET", "/state");
      } catch (e) {
        if (e instanceof ApiError && e.status === 401) {
          setLive((l) => ({ ...l, unauthorized: true }));
          return;
        }
      }
      if (stopped) return;
      const proto = location.protocol === "https:" ? "wss:" : "ws:";
      ws = new WebSocket(`${proto}//${location.host}/ws`);
      ws.onopen = () => {
        attempt = 0;
        setLive((l) => ({ ...l, online: true }));
      };
      ws.onmessage = (ev) => {
        const msg = JSON.parse(ev.data);
        if (msg.type === "state") {
          setLive({ state: msg.data, online: true, unauthorized: false, receivedLocal: Date.now() });
        }
      };
      ws.onclose = () => {
        setLive((l) => ({ ...l, online: false }));
        if (stopped) return;
        const delay = Math.min(MAX_BACKOFF, 500 * 2 ** attempt++);
        timer = window.setTimeout(connect, delay);
      };
    };
    connect();

    // Phones suspend background tabs; reconnect right away when shown again.
    const onVisible = () => {
      if (document.visibilityState === "visible" && ws?.readyState !== WebSocket.OPEN) {
        window.clearTimeout(timer);
        attempt = 0;
        ws?.close();
        connect();
      }
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      stopped = true;
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisible);
      ws?.close();
    };
  }, []);

  return live;
}

/** Re-renders every `ms` milliseconds. */
export function useTick(ms: number) {
  const [, setN] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => setN((n) => n + 1), ms);
    return () => window.clearInterval(id);
  }, [ms]);
}

/** Runs an async action with a pending flag that blocks double taps. */
export function useBusy() {
  const [busy, setBusy] = useState(false);
  const inFlight = useRef(false);
  const run = async (fn: () => Promise<unknown>) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    try {
      await fn();
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  };
  return [busy, run] as const;
}
