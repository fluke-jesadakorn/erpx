/* eslint-disable @next/next/no-img-element -- Private workspace logos are served through authenticated attachment routes. */
import type { WorkspaceBrand } from "@/server/branding";
import styles from "./workspace-brand.module.css";
import { type UiAppearance } from "@/components/ui/foundation";

export function WorkspaceBrandMark({ appearance = "legacy", brand, compact = false }: ({ brand: WorkspaceBrand; compact?: boolean }) & { appearance?: UiAppearance }) {
  return (
    <span data-ui-appearance={appearance} className={`${styles.mark} ${compact ? styles.compact : ""}`} aria-label={`${brand.label} logo`}>
      {brand.primaryUrl ? <img data-ui-appearance={appearance} src={brand.primaryUrl} alt="" /> : <span data-ui-appearance={appearance} aria-hidden="true">{brand.initials}</span>}
    </span>
  );
}
