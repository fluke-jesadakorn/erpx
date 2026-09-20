"use client";
import { scheduleWorkRefresh } from "@/lib/work-refresh";

import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { usePathname } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";

type Counts = { home: number | null; work: number | null };
const empty: Counts = { home: null, work: null };
const NavigationCounts = createContext<{ counts: Counts; readHome: () => void; publishWork: (count: number | null) => void }>({ counts: empty, readHome: () => {}, publishWork: () => {} });
export const useNavigationCounts = () => useContext(NavigationCounts);

export function NavigationCountsProvider({ base, children }: { base: string; children: React.ReactNode }) {
  const pathname = usePathname();
  const home = pathname === `${base}/home` || pathname === `${base}/feed`;
  const [counts, setCounts] = useState<Counts>(empty);
  const generation = useRef(0);
  const pendingRead = useRef<Promise<void> | null>(null);
  const request = useRef<AbortController | null>(null);
  const summaryRevision = useRef(0);
  const invalidate = useCallback(() => { ++generation.current; }, []);
  const publishWork = useCallback((work: number | null) => {
    ++summaryRevision.current;
    setCounts(value => ({ ...value, work }));
  }, []);
  const endpoint = `/api/v1${base}/navigation`;
  const refresh = useCallback(async () => {
    request.current?.abort();
    const controller = new AbortController(); request.current = controller;
    const workRevision = summaryRevision.current;
    const current = ++generation.current;
    try {
      const response = await workFetch(endpoint, { cache: "no-store", signal: controller.signal });
      if (!response.ok) throw new Error("Counts unavailable");
      const { data } = await response.json();
      if (!controller.signal.aborted && current === generation.current) setCounts(previous => ({ home: data.home, work: workRevision === summaryRevision.current ? data.work : previous.work }));
    } catch {
      if (!controller.signal.aborted && current === generation.current) setCounts(empty);
    } finally {
      if (request.current === controller) request.current = null;
    }
  }, [endpoint]);
  const readHome = useCallback(() => {
    if (pendingRead.current) return pendingRead.current;
    ++generation.current;
    setCounts(value => ({ ...value, home: 0 }));
    pendingRead.current = (async () => {
      let saved = false;
      try {
        const response = await workFetch(endpoint, {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ action: "read-home" }),
        });
        if (!response.ok) throw new Error("Unable to mark Home as read");
        const { data } = await response.json() as { data: { home: number } };
        saved = true;
        setCounts(value => ({ ...value, home: data.home }));
      } finally {
        pendingRead.current = null;
        if (!saved) await refresh();
      }
    })().catch(() => {});
    return pendingRead.current;
  }, [endpoint, refresh]);
  useEffect(() => {
    // Read on entry, never on a background poll or when opening a post detail.
    const initial = setTimeout(() => {
      if (home) void readHome().then(refresh);
      else void refresh();
    }, 0);
    const update = () => {
      if (document.visibilityState === "visible" && !pendingRead.current) void refresh();
    };
    const summary = (event: Event) => {
      const work = (event as CustomEvent<{ work?: unknown }>).detail?.work;
      if (work === null || typeof work === "number" && Number.isInteger(work) && work >= 0) {
        publishWork(work);
      }
    };
    const timer = setInterval(update, 30_000);
    const stopRefresh = scheduleWorkRefresh(update);
    window.addEventListener("erpx:work-summary", summary);

    return () => {
      invalidate(); request.current?.abort();
      clearTimeout(initial);
      clearInterval(timer);
      stopRefresh();
      window.removeEventListener("erpx:work-summary", summary);

    };
  }, [home, base, refresh, readHome, invalidate, publishWork]);
  return <NavigationCounts.Provider value={{ counts, readHome, publishWork }}>{children}</NavigationCounts.Provider>;
}
