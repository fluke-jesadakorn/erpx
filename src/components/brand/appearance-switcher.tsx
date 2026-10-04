"use client";
import { Select } from "@/components/ui/foundation";

import { useId, useSyncExternalStore } from "react";
import { appearancePreference, type AppearancePreference } from "@/lib/appearance-preference";
import styles from "./appearance-switcher.module.css";
import { type UiAppearance } from "@/components/ui/foundation";

function subscribe(changed: () => void) {
  window.addEventListener("erpx-appearance", changed);
  return () => window.removeEventListener("erpx-appearance", changed);
}

export function AppearanceSwitcher({ appearance = "legacy",
  initialTheme,
  compact = false,
}: ({
  initialTheme: AppearancePreference;
  compact?: boolean;
}) & { appearance?: UiAppearance }) {
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

  return (
    <div data-ui-appearance={appearance} className={`${styles.switcher} ${compact ? styles.compact : ""}`}>
      <label data-ui-appearance={appearance} htmlFor={id}>Theme</label>
      <Select appearance={appearance}
        id={id}
        value={theme}
        onChange={(event) => change(appearancePreference(event.target.value))}
      >
        <option data-ui-appearance={appearance} value="system">System</option>
        <option data-ui-appearance={appearance} value="light">Light</option>
        <option data-ui-appearance={appearance} value="dark">Dark</option>
      </Select>
    </div>
  );
}
