"use client";
import { Select } from "@erpx/ui/components/foundation";

import { useId, useSyncExternalStore } from "react";
import { appearancePreference, type AppearancePreference } from "@erpx/feature-shared/lib/appearance-preference";
import styles from "@erpx/feature-brand/components/appearance-switcher.module.css";
import { type UiAppearance } from "@erpx/ui/components/foundation";

function subscribe(changed: () => void) {
  window.addEventListener("erpx-appearance", changed);
  return () => window.removeEventListener("erpx-appearance", changed);
}

export function AppearanceSwitcher({ appearance = "legacy",
  initialTheme,
  compact = false,
  labels = {label: "Theme", system: "System", light: "Light", dark: "Dark"},
}: ({
  initialTheme: AppearancePreference;
  compact?: boolean;
  labels?: {label: string; system: string; light: string; dark: string};
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
      <label data-ui-appearance={appearance} htmlFor={id}>{labels.label}</label>
      <Select appearance={appearance}
        id={id}
        value={theme}
        onChange={(event) => change(appearancePreference(event.target.value))}
      >
        <option data-ui-appearance={appearance} value="system">{labels.system}</option>
        <option data-ui-appearance={appearance} value="light">{labels.light}</option>
        <option data-ui-appearance={appearance} value="dark">{labels.dark}</option>
      </Select>
    </div>
  );
}
