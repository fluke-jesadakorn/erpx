/* eslint-disable @next/next/no-img-element -- Private workspace logos are served through authenticated attachment routes. */
import type { WorkspaceBrand } from "@erpx/server-brand/public/branding";
import styles from "@erpx/feature-brand/components/workspace-brand.module.css";
import { type UiAppearance } from "@erpx/ui/components/foundation";

export function WorkspaceBrandMark({ appearance = "legacy", brand, compact = false }: ({ brand: WorkspaceBrand; compact?: boolean }) & { appearance?: UiAppearance }) {
  return (
    <span data-ui-appearance={appearance} className={`${styles.mark} ${compact ? styles.compact : ""}`} aria-label={`${brand.label} logo`}>
      {brand.primaryUrl ? <img data-ui-appearance={appearance} src={brand.primaryUrl} alt="" /> : <span data-ui-appearance={appearance} aria-hidden="true">{brand.initials}</span>}
    </span>
  );
}
