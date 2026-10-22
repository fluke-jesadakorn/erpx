"use client";

import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import type { QuickAccessPreference } from "@erpx/feature-work_projection/lib/quick-access";
import { parseLocalShortcuts, serializeLocalShortcuts } from "@erpx/feature-work_projection/lib/local-shortcuts";

const changed = "erpx:local-shortcuts";
const serverSnapshot = () => null;

export function useLocalShortcutPreference(key: string | undefined, initial: QuickAccessPreference) {
  const subscribe = useCallback((notify: () => void) => {
    if (!key) return () => {};
    const listener = (event: Event) => {
      if (event instanceof StorageEvent && event.key !== key && event.key !== null) return;
      if (event instanceof CustomEvent && event.detail !== key) return;
      notify();
    };
    window.addEventListener("storage", listener);
    window.addEventListener(changed, listener);
    return () => {
      window.removeEventListener("storage", listener);
      window.removeEventListener(changed, listener);
    };
  }, [key]);
  const snapshot = useCallback(() => {
    try { return key ? window.localStorage.getItem(key) : null; }
    catch { return null; }
  }, [key]);
  const raw = useSyncExternalStore(subscribe, snapshot, serverSnapshot);
  const stored = useMemo(() => parseLocalShortcuts(raw), [raw]);
  const save = useCallback((preference: QuickAccessPreference) => {
    if (!key) return;
    window.localStorage.setItem(key, serializeLocalShortcuts(preference));
    window.dispatchEvent(new CustomEvent(changed, { detail: key }));
  }, [key]);

  // Import an existing server arrangement once, without delaying the sidebar.
  // A local save always wins over later server refreshes.
  useEffect(() => {
    if (key && !stored && initial.version > 0) {
      try { save(initial); } catch { /* The save action reports storage failures. */ }
    }
  }, [key, stored, initial, save]);
  return { preference: stored ?? initial, save };
}
