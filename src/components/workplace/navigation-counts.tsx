"use client";
import { scheduleWorkRefresh } from "@/lib/work-refresh";

import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { usePathname } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { WorkViewAvailability } from "@/lib/work-item";
import type { WorkActionCard, WorkCardPreferences } from "@/lib/work-cards";

type Counts = { home: number | null; work: number | null };
type QuickAccessProjection = {
  cards: WorkActionCard[];
  preferences: WorkCardPreferences;
  preferencesUnavailable: boolean;
};
type NavigationContext = {
  counts: Counts;
  workViewAvailability: WorkViewAvailability | null;
  quickAccess: QuickAccessProjection | null;
  quickAccessLoading: boolean;
  quickAccessError: string;
  shortcutStorageKey?: string;
  refresh: () => Promise<void>;
  saveQuickAccess: (preferences: WorkCardPreferences) => void;
  readHome: () => void;
  publishWork: (count: number | null, availability?: WorkViewAvailability) => void;
};
const empty: Counts = { home: null, work: null };
const NavigationCounts = createContext<NavigationContext>({ counts: empty, workViewAvailability: null, quickAccess: null, quickAccessLoading: true, quickAccessError: "", refresh: async () => {}, saveQuickAccess: () => {}, readHome: () => {}, publishWork: () => {} });
export const useNavigationCounts = () => useContext(NavigationCounts);

export function NavigationCountsProvider({ base, initialShortcutCards, shortcutStorageKey, children }: {
  base: string;
  initialShortcutCards: WorkActionCard[];
  shortcutStorageKey: string;
  children: React.ReactNode;
}) {
  const pathname = usePathname();
  const home = pathname === `${base}/home` || pathname === `${base}/feed`;
  const [workViewAvailability, setWorkViewAvailability] = useState<WorkViewAvailability | null>(null);
  const [counts, setCounts] = useState<Counts>(empty);
  const [quickAccess, setQuickAccess] = useState<QuickAccessProjection | null>({
    cards: initialShortcutCards,
    preferences: { pinnedIds: null, version: 0 },
    preferencesUnavailable: false,
  });
  // The initial authorized catalogue has no persisted preference snapshot yet.
  const [quickAccessLoading, setQuickAccessLoading] = useState(true);
  const [quickAccessError, setQuickAccessError] = useState("");
  const generation = useRef(0);
  const pendingRead = useRef<Promise<void> | null>(null);
  const request = useRef<AbortController | null>(null);
  const summaryRevision = useRef(0);
  const invalidate = useCallback(() => { ++generation.current; }, []);
  const publishWork = useCallback((work: number | null, availability?: WorkViewAvailability) => {
    if (availability) setWorkViewAvailability(availability);
    ++summaryRevision.current;
    setCounts(value => ({ ...value, work }));
  }, []);
  const saveQuickAccess = useCallback((preferences: WorkCardPreferences) => {
    setQuickAccess((current) => current ? { ...current, preferences, preferencesUnavailable: false } : current);
    setQuickAccessLoading(false);
    setQuickAccessError("");
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
      const { data } = await response.json() as { data: { home: number; work: number | null; workViewAvailability?: WorkViewAvailability; quickAccess: QuickAccessProjection } };
      if (!controller.signal.aborted && current === generation.current) {
        setCounts(previous => ({ home: data.home, work: workRevision === summaryRevision.current ? data.work : previous.work }));
        if (workRevision === summaryRevision.current && data.workViewAvailability) setWorkViewAvailability(data.workViewAvailability);
        setQuickAccess((previous) => previous && previous.preferences.version > data.quickAccess.preferences.version
          ? { ...data.quickAccess, preferences: previous.preferences }
          : data.quickAccess);
        setQuickAccessLoading(false);
        setQuickAccessError("");
      }
    } catch {
      if (!controller.signal.aborted && current === generation.current) {
        setCounts(empty);
        setWorkViewAvailability(null);
        setQuickAccessLoading(false);
        setQuickAccessError("Shortcuts could not refresh.");
      }
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
      if (home) {
        // Do not hold the shared shortcut shelf behind the Home read command.
        void readHome();
        void refresh();
      } else void refresh();
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
  return <NavigationCounts.Provider value={{ counts, workViewAvailability, quickAccess, quickAccessLoading, quickAccessError, shortcutStorageKey, refresh, saveQuickAccess, readHome, publishWork }}>{children}</NavigationCounts.Provider>;
}
