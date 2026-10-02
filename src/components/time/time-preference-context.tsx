"use client";

import { createContext, useContext, useEffect, useMemo, useState } from "react";
import { defaultTimePreference, formatPreferredDateTime, resolvedTimeZone, systemTimeZone, type TimePreference } from "@/lib/ui-format";
import { useHydrated } from "@/components/ui/use-hydrated";
import { usePathname } from "next/navigation";

type TimeContextValue = {
  preference: TimePreference;
  systemZone: string;
  ready: boolean;
  setPreference: (next: TimePreference) => void;
};

const TimeContext = createContext<TimeContextValue>({
  preference: defaultTimePreference, systemZone: "UTC", ready: false, setPreference: () => {},
});

export function TimePreferenceProvider({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const hydrated = useHydrated();
  const [preference, setPreference] = useState<TimePreference>(defaultTimePreference);
  const [systemZone, setSystemZone] = useState("UTC");
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    let active = true;
    const refreshZone = () => { if (active) setSystemZone(systemTimeZone()); };
    refreshZone();
    const channel = typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel("erpx-time-preference");
    const refreshPreference = () => fetch("/api/v1/account/time-preference", { cache: "no-store" })
      .then(response => response.ok ? response.json() : null)
      .then(body => { if (active) { if (body?.data) setPreference(body.data); setLoaded(true); } })
      .catch(() => { if (active) setLoaded(true); });
    void refreshPreference();
    channel?.addEventListener("message", refreshPreference);
    window.addEventListener("focus", refreshZone);
    document.addEventListener("visibilitychange", refreshZone);
    return () => {
      active = false; channel?.close(); window.removeEventListener("focus", refreshZone);
      document.removeEventListener("visibilitychange", refreshZone);
    };
  }, [pathname]);
  const value = useMemo(() => ({ preference, systemZone, ready: hydrated && loaded, setPreference }), [preference, systemZone, hydrated, loaded]);
  return <TimeContext.Provider value={value}>{children}</TimeContext.Provider>;
}

export function useTimePreference() { return useContext(TimeContext); }

export function TimeText({ value, options, className }: {
  value: Date | number | string;
  options?: Intl.DateTimeFormatOptions;
  className?: string;
}) {
  const { preference, systemZone, ready } = useTimePreference();
  const date = value instanceof Date ? value : new Date(value);
  return <time className={className} dateTime={Number.isFinite(date.getTime()) ? date.toISOString() : undefined}>
    {ready && Number.isFinite(date.getTime()) ? formatPreferredDateTime(date, preference, systemZone, options) : "…"}
  </time>;
}

export function useDisplayTimeZone() {
  const { preference, systemZone, ready } = useTimePreference();
  return { timeZone: resolvedTimeZone(preference, systemZone), ready, hourCycle: preference.hourCycle };
}
