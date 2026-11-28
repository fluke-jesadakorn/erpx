import { cva } from "class-variance-authority";

// Adapted from the pinned shadcn base-nova source in vendor/shadcn.
// Native layout buttons retain their own layout while sharing focus/disabled
// behavior. Business actions use the primary/secondary/ghost/danger variants.
export const buttonVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 rounded-[var(--ui-radius-md)] border border-transparent text-sm font-semibold whitespace-nowrap transition-colors disabled:cursor-not-allowed disabled:opacity-55 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--ui-brand)] motion-reduce:transition-none [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary: "bg-[var(--ui-brand)] text-[var(--ui-on-brand)] hover:bg-[var(--ui-brand-strong)]",
        secondary:
          "border-[var(--ui-line)] bg-[var(--ui-card)] text-[var(--ui-ink)] hover:bg-[var(--ui-card-2)]",
        ghost:
          "bg-transparent text-[var(--ui-muted)] hover:bg-[var(--ui-card-2)] hover:text-[var(--ui-ink-strong)]",
        danger: "bg-[var(--ui-red)] text-white hover:opacity-90",
        plain: "shrink whitespace-normal",
      },
      size: {
        compact: "min-h-9 px-3 py-1.5",
        default: "min-h-[var(--ui-control-height)] px-4 py-2",
        spacious: "min-h-[var(--ui-control-height-spacious)] px-5 py-2.5",
        layout: "",
      },
    },
    compoundVariants: [{ variant: "plain", class: "p-0" }],
    defaultVariants: { variant: "primary", size: "default" },
  },
);

export type UiButtonVariant = "primary" | "secondary" | "ghost" | "danger" | "plain";
export type UiSize = "compact" | "default" | "spacious";
