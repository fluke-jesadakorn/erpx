"use client";
import type { NavigationSnapshot } from "@/server/modules/work_projection/public/navigation-snapshot";
import { contextIsSwitching } from "@/lib/work_projection/context-switch";
import { workAttention, type WorkAttention } from "@/lib/work_projection/work-attention";
import { scheduleWorkRefresh } from "@/lib/work_projection/work-refresh";

import { createContext, use, useCallback, useContext, useEffect, useRef, useState } from "react";
import { usePathname } from "next/navigation";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { WorkViewAvailability } from "@/lib/work_projection/work-item";

type Counts = { home: number | null; work: number | null };
type NavigationContext = {
  initial?: Promise<NavigationSnapshot | null>;
  initialPending?: boolean;
  homeChanged?: boolean;
  workChanged?: boolean;
  counts: Counts;
  attention: WorkAttention;
  snapshotAt: number;
  updating: boolean;
  error: boolean;
  workViewAvailability: WorkViewAvailability | null;
  shortcutStorageKey?: string;
  refresh: () => Promise<void>;
  readHome: () => void;
  publishWork: (count: number | null, availability?: WorkViewAvailability) => void;
};
const empty: Counts = { home: null, work: null };
const NavigationCounts = createContext<NavigationContext>({ attention: workAttention([]), snapshotAt: 0, updating: true, error: false, counts: empty, workViewAvailability: null, refresh: async () => {}, readHome: () => {}, publishWork: () => {} });
/** Actions and the menu shell never suspend on badge data. */
export const useNavigationState = () => useContext(NavigationCounts);
export function useNavigationCounts() {
  const state = useNavigationState();
  const initial = state.initialPending && state.initial ? use(state.initial) : null;
  if (!initial) return state;
  return { ...state, counts: {
    home: state.homeChanged ? state.counts.home : initial.home,
    work: state.workChanged ? state.counts.work : initial.work,
  }, attention: initial.attention ?? workAttention([]), snapshotAt: initial.snapshotAt ?? 0,
  workViewAvailability: state.workChanged ? state.workViewAvailability : initial.workViewAvailability,
  updating: false };
}

export function NavigationCountsProvider({ base, shortcutStorageKey, initial, children }: {
  base: string;
  initial?: Promise<NavigationSnapshot | null>;
  shortcutStorageKey: string;
  children: React.ReactNode;
}) {
  const [homeChanged, setHomeChanged] = useState(false);
  const [workChanged, setWorkChanged] = useState(false);
  const [initialPending, setInitialPending] = useState(!!initial);
  const pathname = usePathname();
  const home = pathname === `${base}/home` || pathname === `${base}/feed`;
  const [workViewAvailability, setWorkViewAvailability] = useState<WorkViewAvailability | null>(null);
  const [counts, setCounts] = useState<Counts>(empty);
  const [attention, setAttention] = useState(() => workAttention([]));
  const [snapshotAt, setSnapshotAt] = useState(0);
  const [updating, setUpdating] = useState(true);
  const [error, setError] = useState(false);
  const stale = useRef(false);
  const initialActive = useRef(!!initial);
  const pendingRefresh = useRef<Promise<void> | null>(null);
  const generation = useRef(0);
  const pendingRead = useRef<Promise<void> | null>(null);
  const request = useRef<AbortController | null>(null);
  const homeRevision = useRef(0);
  const summaryRevision = useRef(0);
  const invalidate = useCallback(() => { ++generation.current; }, []);
  const publishWork = useCallback((work: number | null, availability?: WorkViewAvailability) => {
    if (stale.current) return;
    if (availability) setWorkViewAvailability(availability);
    ++summaryRevision.current;
    setWorkChanged(true);
    setCounts(value => ({ ...value, work }));
  }, []);
  const endpoint = `/api/v1${base}/navigation`;
  const refresh = useCallback(async () => {
    if (stale.current || contextIsSwitching()) return;
    if (pendingRefresh.current) return pendingRefresh.current;
    initialActive.current = false;
    setInitialPending(false);
    setUpdating(true);
    pendingRefresh.current = (async () => {
      const controller = new AbortController(); request.current = controller;
      const workRevision = summaryRevision.current;
      const readRevision = homeRevision.current;
      const current = ++generation.current;
      try {
        const response = await workFetch(`${endpoint}?include=attention`, { cache: "no-store", signal: controller.signal });
        if ([401,403,404,409].includes(response.status)) {
          if (!controller.signal.aborted && current === generation.current) {
            stale.current = true;
            setCounts(empty); setAttention(workAttention([], true)); setSnapshotAt(0); setWorkViewAvailability(null);
          }
        }
        if (!response.ok) throw new Error("Counts unavailable");
        const { data } = await response.json() as { data: { home: number; work: number | null; workViewAvailability?: WorkViewAvailability; attention: WorkAttention; snapshotAt: number } };
        if (!controller.signal.aborted && current === generation.current) {
          setAttention(data.attention);
          setSnapshotAt(data.snapshotAt);
          setError(false);
          setCounts(previous => ({ home: readRevision === homeRevision.current ? data.home : previous.home, work: workRevision === summaryRevision.current ? data.work : previous.work }));
          if (workRevision === summaryRevision.current && data.workViewAvailability) setWorkViewAvailability(data.workViewAvailability);
        }
      } catch {
        if (!controller.signal.aborted && current === generation.current) {
          setError(true);
        }
      } finally {
        if (request.current === controller) { request.current = null; setUpdating(false); }
      }
    })();
    const reading = pendingRefresh.current;
    try { await reading; } finally { if (pendingRefresh.current === reading) pendingRefresh.current = null; }
  }, [endpoint]);
  const readHome = useCallback(() => {
    if (pendingRead.current) return pendingRead.current;
    if (stale.current || contextIsSwitching()) return;
    ++homeRevision.current;
    setHomeChanged(true);
    setCounts(value => ({ ...value, home: 0 }));
    pendingRead.current = (async () => {
      let saved = false;
      try {
        // Mark the badge immediately, but let the initial revision-checked read
        // finish before writing the cursor that would invalidate its fence.
        if (initial) await initial;
        if (stale.current || contextIsSwitching()) return;
        const response = await workFetch(endpoint, {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ action: "read-home" }),
        });
        if (!response.ok) throw new Error("Unable to mark Home as read");
        const { data } = await response.json() as { data: { home: number } };
        saved = true;
        if (!stale.current) setCounts(value => ({ ...value, home: data.home }));
      } finally {
        pendingRead.current = null;
        if (!saved) await refresh();
      }
    })().catch(() => {});
    return pendingRead.current;
  }, [endpoint, initial, refresh]);
  useEffect(() => {
    let active = true;
    const current = generation.current;
    // Capture field revisions before readHome / Work publishers run. A delayed
    // server result must never replace a newer local write or refresh.
    const workRevision = 0;
    const readRevision = 0;
    if (initial) void initial.then(data => {
      if (!active || !initialActive.current || stale.current || current !== generation.current) return;
      initialActive.current = false;
      setInitialPending(false);
      if (!data) { void refresh(); return; }
      setAttention(data.attention ?? workAttention([]));
      setSnapshotAt(data.snapshotAt ?? 0);
      setCounts(previous => ({ home: readRevision === homeRevision.current ? data.home : previous.home,
        work: workRevision === summaryRevision.current ? data.work : previous.work }));
      if (workRevision === summaryRevision.current) setWorkViewAvailability(data.workViewAvailability);
      setUpdating(false);
    });
    const firstRefresh = initial ? undefined : setTimeout(() => void refresh(), 0);
    const update = () => {
      if (document.visibilityState === "visible" && !pendingRead.current) void refresh();
    };
    const clearContext = () => {
      stale.current = true;
      initialActive.current = false;
      setInitialPending(false);
      invalidate(); request.current?.abort();
      setCounts(empty); setAttention(workAttention([], true)); setSnapshotAt(0); setWorkViewAvailability(null); setUpdating(false);
    };
    window.addEventListener("erpx:context-switch-started", clearContext);
    window.addEventListener("erpx:work-context-changed", clearContext);
    const summary = (event: Event) => {
      const work = (event as CustomEvent<{ work?: unknown }>).detail?.work;
      if (work === null || typeof work === "number" && Number.isInteger(work) && work >= 0) {
        publishWork(work);
      }
    };
    const timer = setInterval(update, 30_000);
    const stopRefresh = scheduleWorkRefresh(update, ["focus", "visibilitychange"]);
    const stopChanges = scheduleWorkRefresh(() => {
      void (async () => { await pendingRefresh.current; await refresh(); })();
    }, ["erpx:work-changed"]);
    window.addEventListener("erpx:work-summary", summary);

    return () => {
      active = false;
      invalidate(); request.current?.abort(); pendingRefresh.current = null;
      clearTimeout(firstRefresh);
      clearInterval(timer);
      stopRefresh();
      stopChanges();
      window.removeEventListener("erpx:work-summary", summary);
      window.removeEventListener("erpx:context-switch-started", clearContext);
      window.removeEventListener("erpx:work-context-changed", clearContext);

    };
  }, [base, initial, refresh, invalidate, publishWork]);
  useEffect(() => {
    if (!home) return;
    // Entry marks the feed read; polling leaves unread state alone.
    const timer = setTimeout(() => void readHome(), 0);
    return () => clearTimeout(timer);
  }, [home, readHome]);
  return <NavigationCounts.Provider value={{ initial, initialPending, homeChanged, workChanged, attention, snapshotAt, updating, error, counts, workViewAvailability, shortcutStorageKey, refresh, readHome, publishWork }}>{children}</NavigationCounts.Provider>;
}
