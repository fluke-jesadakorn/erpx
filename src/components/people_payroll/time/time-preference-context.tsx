"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
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

export function TimePreferenceProvider({ children, initialPreference, accountKey = "default" }: ({ children: React.ReactNode; initialPreference?: TimePreference; accountKey?: string }) & { appearance?: UiAppearance }) {
  const pathname = usePathname();
  const hydrated = useHydrated();
  const [received, receivePreference] = useState<{ accountKey: string; preference: TimePreference } | null>(null);
  const receivedPreference = received?.accountKey === accountKey ? received.preference : null;
  const seeded = initialPreference !== undefined;
  const preference = receivedPreference && (!initialPreference || receivedPreference.version >= initialPreference.version)
    ? receivedPreference : initialPreference ?? defaultTimePreference;
  const setPreference = useCallback((next: TimePreference) => receivePreference(current => current?.accountKey === accountKey && current.preference.version > next.version ? current : { accountKey, preference: next }), [accountKey]);
  const pendingRead = useRef<{ accountKey: string; read: Promise<TimePreference | null> } | null>(null);
  const [systemZone, setSystemZone] = useState("UTC");
  const [loadedFor, setLoadedFor] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    const refreshZone = () => { if (active) setSystemZone(systemTimeZone()); };
    refreshZone();
    const channel = typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel("erpx-time-preference");
    const refreshPreference = () => {
      // Share only an in-flight read within this mounted account provider. In
      // particular, development Strict Mode must not issue two identical GETs.
      if (!pendingRead.current || pendingRead.current.accountKey !== accountKey) {
        const read = fetch("/api/v1/account/time-preference", { cache: "no-store" })
          .then(response => response.ok ? response.json() : null)
          .then(body => body?.data as TimePreference | undefined ?? null).catch(() => null);
        const pending = { accountKey, read };
        pendingRead.current = pending;
        void read.finally(() => { if (pendingRead.current === pending) pendingRead.current = null; });
      }
      void pendingRead.current.read.then(next => {
        if (active) {
          if (next) receivePreference(current => current?.accountKey === accountKey && current.preference.version > next.version ? current : { accountKey, preference: next });
          setLoadedFor(accountKey);
        }
      });
    };
    if (!seeded) refreshPreference();
    channel?.addEventListener("message", refreshPreference);
    window.addEventListener("focus", refreshZone);
    document.addEventListener("visibilitychange", refreshZone);
    return () => {
      active = false; channel?.close(); window.removeEventListener("focus", refreshZone);
      document.removeEventListener("visibilitychange", refreshZone);
    };
  }, [pathname, seeded, accountKey]);
  const value = useMemo(() => ({ preference, systemZone, ready: hydrated && (seeded || loadedFor === accountKey), setPreference }), [preference, systemZone, hydrated, seeded, loadedFor, accountKey, setPreference]);
  return <TimeContext.Provider value={value}>{children}</TimeContext.Provider>;
}

export function useTimePreference() { return useContext(TimeContext); }

export function TimeText({ value, options, className, appearance = "legacy" }: ({
  value: Date | number | string;
  options?: Intl.DateTimeFormatOptions;
  className?: string;
}) & { appearance?: UiAppearance }) {
  const { preference, systemZone, ready } = useTimePreference();
  const hydrated = useHydrated();
  const date = value instanceof Date ? value : new Date(value);
  return <time data-ui-appearance={appearance} className={className} dateTime={Number.isFinite(date.getTime()) ? date.toISOString() : undefined}>
    {hydrated && ready && Number.isFinite(date.getTime()) ? formatPreferredDateTime(date, preference, systemZone, options) : "…"}
  </time>;
}

export function useDisplayTimeZone() {
  const { preference, systemZone, ready } = useTimePreference();
  return { timeZone: resolvedTimeZone(preference, systemZone), ready, hourCycle: preference.hourCycle };
}
