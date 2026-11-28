"use client";
import { useNavigationCounts } from "@erpx/platform-workplace/components/navigation-counts";
import styles from "@erpx/platform-workplace/components/shell.module.css";

export function NavigationBadge({ kind }: { kind: "home" | "work" }) {
  const count = useNavigationCounts().counts[kind];
  if (kind === "home" && count === 0) return null;
  return <span className={styles.navBadge}
    title={kind === "work" ? "My actions; – means incomplete" : "New team updates"}>
    {count ?? "–"}
  </span>;
}
