"use client";

import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { usePathname } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";

type Counts = { home: number | null; work: number | null };
const empty: Counts = { home: null, work: null };
const NavigationCounts = createContext({ counts: empty, readHome: () => {} });
export const useNavigationCounts = () => useContext(NavigationCounts);

export function NavigationCountsProvider({ base, children }: { base: string; children: React.ReactNode }) {
  const pathname = usePathname();
  const [counts, setCounts] = useState<Counts>(empty);
  const generation = useRef(0);
  const pendingRead = useRef<Promise<void> | null>(null);
  const fetching = useRef(false);
  const invalidate = useCallback(() => { ++generation.current; }, []);
  const endpoint = `/api/v1${base}/navigation`;
  const refresh = useCallback(async () => {
    if (fetching.current) return;
    fetching.current = true;
    const current = ++generation.current;
    try {
      const response = await workFetch(endpoint, { cache: "no-store" });
      if (!response.ok) throw new Error("Counts unavailable");
      const { data } = await response.json();
      if (current === generation.current) setCounts(data);
    } catch {
      if (current === generation.current) setCounts(empty);
    } finally {
      fetching.current = false;
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
    const home = pathname === `${base}/home` || pathname === `${base}/feed`;
    // Read on entry, never on a background poll or when opening a post detail.
    const initial = setTimeout(() => {
      if (home) void readHome();
      else void refresh();
    }, 0);
    const update = () => {
      if (!home && document.visibilityState === "visible" && !pendingRead.current) void refresh();
    };
    const summary = (event: Event) => {
      const work = (event as CustomEvent<{ work?: unknown }>).detail?.work;
      if (typeof work === "number" && Number.isInteger(work) && work >= 0)
        setCounts(value => ({ ...value, work }));
    };
    const timer = setInterval(update, 30_000);
    window.addEventListener("focus", update);
    window.addEventListener("erpx:work-summary", summary);
    document.addEventListener("visibilitychange", update);
    return () => {
      invalidate();
      clearTimeout(initial);
      clearInterval(timer);
      window.removeEventListener("focus", update);
      window.removeEventListener("erpx:work-summary", summary);
      document.removeEventListener("visibilitychange", update);
    };
  }, [pathname, base, refresh, readHome, invalidate]);
  return <NavigationCounts.Provider value={{ counts, readHome }}>{children}</NavigationCounts.Provider>;
}
