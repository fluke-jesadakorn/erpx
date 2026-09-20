import type { ReactNode } from "react";
import styles from "./home-visual-prototype.module.css";

/** The workspace home has a more expressive hierarchy than record pages. */
export function HomeVisualPrototype({
  surface,
  children,
}: {
  surface: "workspace" | "platform";
  children: ReactNode;
}) {
  if (surface === "platform") return children;

  return (
    <div className={styles.workspace} data-home-surface="workspace">
      {children}
    </div>
  );
}
