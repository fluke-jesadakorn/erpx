/* eslint-disable @next/next/no-img-element -- Private workspace logos are served through authenticated attachment routes. */
import type { WorkspaceBrand } from "@/server/branding";
import styles from "./workspace-brand.module.css";

export function WorkspaceBrandMark({ brand, compact = false }: { brand: WorkspaceBrand; compact?: boolean }) {
  return (
    <span className={`${styles.mark} ${compact ? styles.compact : ""}`} aria-label={`${brand.label} logo`}>
      {brand.primaryUrl ? <img src={brand.primaryUrl} alt="" /> : <span aria-hidden="true">{brand.initials}</span>}
    </span>
  );
}
