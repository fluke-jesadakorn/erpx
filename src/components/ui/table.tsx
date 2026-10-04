import type { HTMLAttributes, TableHTMLAttributes, ReactNode } from "react";
import type { UiAppearance } from "./appearance";
import { cn } from "@/lib/ui";

export function TableFrame({
  label,
  children,
  density = "comfortable",
  appearance = "legacy",
  className,
  ...props
}: HTMLAttributes<HTMLDivElement> & {
  label: string;
  children: ReactNode;
  density?: "compact" | "comfortable";
  appearance?: UiAppearance;
}) {
  return (
    <div
      data-slot="table-frame"
      data-density={density}
      data-appearance={appearance}
      role="region"
      aria-label={label}
      tabIndex={0}
      className={cn(
        "max-w-full overflow-auto rounded-[var(--ui-radius-lg)] border border-[var(--ui-line)] bg-[var(--ui-card)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--ui-brand)]",
        appearance === "standard" && "ui-standard-table",
        className,
      )}
      {...props}
    >
      {children}
    </div>
  );
}

export function Table({ className, ...props }: TableHTMLAttributes<HTMLTableElement>) {
  return (
    <table
      data-slot="table"
      className={cn("w-full border-collapse text-left text-sm", className)}
      {...props}
    />
  );
}
