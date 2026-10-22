import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "@erpx/ui/lib/cn";
import type { UiAppearance } from "@erpx/ui/components/appearance";

export type UiDensity = "compact" | "comfortable";

export function Card({
  children,
  density = "comfortable",
  className,
  as: Element = "section",
  appearance = "legacy",
  ...props
}: HTMLAttributes<HTMLElement> & {
  children: ReactNode;
  density?: UiDensity;
  appearance?: UiAppearance;
  className?: string;
  as?: "article" | "div" | "section";
}) {
  return (
    <Element
      {...props}
      data-slot="card"
      data-appearance={appearance}
      className={cn(
        "min-w-0 rounded-[var(--ui-radius-lg)] border border-[var(--ui-line)] bg-[var(--ui-card)]",
        appearance === "standard" ? "ui-panel" : density === "compact" ? "p-4" : "p-4 sm:p-6",
        className,
      )}
    >
      {children}
    </Element>
  );
}
