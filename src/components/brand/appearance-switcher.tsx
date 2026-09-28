"use client";

import { useId, useSyncExternalStore } from "react";
import { appearancePreference, type AppearancePreference } from "@/lib/appearance-preference";
import styles from "./appearance-switcher.module.css";

function subscribe(changed: () => void) {
  window.addEventListener("erpx-appearance", changed);
  return () => window.removeEventListener("erpx-appearance", changed);
}

export function AppearanceSwitcher({ initialTheme, compact = false }: { initialTheme: AppearancePreference; compact?: boolean }) {
  const id = useId();
  const theme = useSyncExternalStore(
    subscribe,
    () => appearancePreference(document.documentElement.getAttribute("data-theme") ?? undefined),
    () => initialTheme,
  );

  function change(value: AppearancePreference) {
    document.documentElement.setAttribute("data-theme", value);
    document.cookie = `erpx_appearance=${value}; Path=/; Max-Age=31536000; SameSite=Lax${location.protocol === "https:" ? "; Secure" : ""}`;
    window.dispatchEvent(new Event("erpx-appearance"));
  }

  return <div className={`${styles.switcher} ${compact ? styles.compact : ""}`}>
    <label htmlFor={id}>Theme</label>
    <select id={id} value={theme} onChange={event => change(appearancePreference(event.target.value))}>
      <option value="system">System</option>
      <option value="light">Light</option>
      <option value="dark">Dark</option>
    </select>
  </div>;
}
