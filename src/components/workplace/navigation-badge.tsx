"use client";
import { useNavigationCounts } from "./navigation-counts";
import styles from "./shell.module.css";

export function NavigationBadge({ kind }: { kind: "home" | "work" }) {
  const count = useNavigationCounts().counts[kind];
  if (kind === "home" && count === 0) return null;
  return <span className={styles.navBadge}
    title={kind === "work" ? "My actions; – means incomplete" : "New team updates"}>
    {count ?? "–"}
  </span>;
}
