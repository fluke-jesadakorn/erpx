"use client";

import { createContext, useContext, useEffect, useMemo, useState } from "react";
import { defaultTimePreference, formatPreferredDateTime, resolvedTimeZone, systemTimeZone, type TimePreference } from "@/lib/shared/ui-format";
import { useHydrated } from "@/components/ui/use-hydrated";
import { usePathname } from "next/navigation";
import { type UiAppearance } from "@/components/ui/foundation";

type TimeContextValue = {
  preference: TimePreference;
  systemZone: string;
  ready: boolean;
  setPreference: (next: TimePreference) => void;
};

const TimeContext = createContext<TimeContextValue>({
  preference: defaultTimePreference, systemZone: "UTC", ready: false, setPreference: () => {},
});

export function TimePreferenceProvider({ children }: ({ children: React.ReactNode }) & { appearance?: UiAppearance }) {
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

export function TimeText({ value, options, className, appearance = "legacy" }: ({
  value: Date | number | string;
  options?: Intl.DateTimeFormatOptions;
  className?: string;
}) & { appearance?: UiAppearance }) {
  const { preference, systemZone, ready } = useTimePreference();
  const date = value instanceof Date ? value : new Date(value);
  return <time data-ui-appearance={appearance} className={className} dateTime={Number.isFinite(date.getTime()) ? date.toISOString() : undefined}>
    {ready && Number.isFinite(date.getTime()) ? formatPreferredDateTime(date, preference, systemZone, options) : "…"}
  </time>;
}

export function useDisplayTimeZone() {
  const { preference, systemZone, ready } = useTimePreference();
  return { timeZone: resolvedTimeZone(preference, systemZone), ready, hourCycle: preference.hourCycle };
}
