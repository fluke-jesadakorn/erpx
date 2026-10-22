import type { ReactNode } from "react";
import styles from "@erpx/feature-work_projection/components/home/home-visual-prototype.module.css";

/** The workspace home has a more expressive hierarchy than record pages. */
export function HomeVisualPrototype({
  surface,
  compactHero = false,
  children,
}: {
  surface: "workspace" | "platform";
  compactHero?: boolean;
  children: ReactNode;
}) {
  if (surface === "platform") return children;

  return (
    <div className={styles.workspace} data-home-surface="workspace" data-compact-hero={compactHero || undefined}>
      {children}
    </div>
  );
}
