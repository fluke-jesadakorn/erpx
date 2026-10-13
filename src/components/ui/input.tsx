import {
  forwardRef,
  type InputHTMLAttributes,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";
import { cn } from "@/lib/shared/ui";
import type { UiAppearance } from "./appearance";
import type { UiSize } from "./button-variants";

const control =
  "w-full min-w-0 rounded-[var(--ui-radius-md)] border border-[var(--ui-control-border)] bg-[var(--ui-card)] px-3 text-sm text-[var(--ui-ink)] transition-colors disabled:bg-[var(--ui-card-2)] disabled:text-[var(--ui-muted)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--ui-brand)] aria-invalid:border-[var(--ui-red)] motion-reduce:transition-none";
const sizes = {
  compact: "min-h-9",
  default: "min-h-[var(--ui-control-height)]",
  spacious: "min-h-[var(--ui-control-height-spacious)]",
};

// Native controls keep FormData, browser validation, onChange and ref contracts
// intact in server-rendered forms; complex interaction is owned by Base UI.
export const TextInput = forwardRef<
  HTMLInputElement,
  InputHTMLAttributes<HTMLInputElement> & {
    invalid?: boolean;
    appearance?: UiAppearance;
    inputSize?: UiSize;
  }
>(function TextInput({ invalid, appearance = "legacy", inputSize = "default", className, type, ...props }, ref) {
  const choice = type === "checkbox" || type === "radio";
  const styled = type !== "hidden" && type !== "file" && !choice;
  return (
    <input
      ref={ref}
      type={type}
      data-slot={choice ? type : "input"}
      data-appearance={appearance === "standard" ? appearance : undefined}
      data-size={inputSize}
      className={cn(styled && control, styled && sizes[inputSize], styled && appearance === "standard" && "ui-standard-control", className)}
      aria-invalid={invalid || undefined}
      {...props}
    />
  );
});

export const Select = forwardRef<
  HTMLSelectElement,
  SelectHTMLAttributes<HTMLSelectElement> & {
    invalid?: boolean;
    appearance?: UiAppearance;
    inputSize?: UiSize;
  }
>(function Select({ invalid, appearance = "legacy", inputSize = "default", className, children, ...props }, ref) {
  return (
    <select
      ref={ref}
      data-slot="select"
      data-appearance={appearance === "standard" ? appearance : undefined}
      data-size={inputSize}
      className={cn(control, sizes[inputSize], appearance === "standard" && "ui-standard-control", className)}
      aria-invalid={invalid || undefined}
      {...props}
    >
      {children}
    </select>
  );
});

export const TextArea = forwardRef<
  HTMLTextAreaElement,
  TextareaHTMLAttributes<HTMLTextAreaElement> & {
    invalid?: boolean;
    appearance?: UiAppearance;
  }
>(function TextArea({ invalid, appearance = "legacy", className, ...props }, ref) {
  return (
    <textarea
      ref={ref}
      data-slot="textarea"
      data-appearance={appearance === "standard" ? appearance : undefined}
      className={cn(control, "min-h-28 resize-y py-2.5", appearance === "standard" && "ui-standard-control", className)}
      aria-invalid={invalid || undefined}
      {...props}
    />
  );
});
