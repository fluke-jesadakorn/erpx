import { forwardRef, type LabelHTMLAttributes, type ReactNode } from "react";
import { cn } from "@/lib/ui";

import type { UiAppearance } from "./appearance";

export type FieldProps = LabelHTMLAttributes<HTMLLabelElement> & {
  label?: string;
  appearance?: UiAppearance;
  hint?: string;
  error?: string;
  required?: boolean;
  orientation?: "vertical" | "inline";
  children?: ReactNode;
};

// A wrapping native label also associates controls without client hydration.
export const Field = forwardRef<HTMLLabelElement, FieldProps>(function Field(
  { label, hint, error, required, appearance = "legacy", orientation = "vertical", children, className, ...props },
  ref,
) {
  return (
    <label
      ref={ref}
      data-slot="field"
      className={cn(
        "min-w-0 gap-2 text-sm text-[var(--ui-ink)]",
        orientation === "inline" ? "flex min-h-11 items-center" : "grid",
        appearance === "standard" && "ui-standard-field",
        className,
      )}
      {...props}
    >
      {label && (
        <span className="font-semibold" data-required={required || undefined}>
          {label}
          {required && (
            <span aria-hidden="true" className="text-[var(--ui-red)]">
              {" "}
              *
            </span>
          )}
        </span>
      )}
      {children}
      {error ? (
        <span className="text-xs text-[var(--ui-red)]" role="alert">
          {error}
        </span>
      ) : hint ? (
        <span className="text-xs font-normal text-[var(--ui-muted)]">{hint}</span>
      ) : null}
    </label>
  );
});
