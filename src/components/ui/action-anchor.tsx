import type { AnchorHTMLAttributes } from "react";
import { cn } from "@/lib/ui";
import { buttonVariants, type UiButtonVariant, type UiSize } from "./button-variants";
import type { UiAppearance } from "./appearance";

export type ActionAnchorProps = AnchorHTMLAttributes<HTMLAnchorElement> & {
  variant?: UiButtonVariant;
  appearance?: UiAppearance;
  size?: UiSize;
};

export function ActionAnchor({
  children,
  variant = "secondary",
  appearance = "legacy",
  size = "default",
  className,
  ...props
}: ActionAnchorProps) {
  return (
    <a
      data-slot="button"
      data-size={size}
      data-variant={variant}
      data-appearance={appearance === "standard" ? appearance : undefined}
      className={cn(buttonVariants({ variant, size }), appearance === "standard" && "ui-standard-control", className)}
      {...props}
    >
      {children}
    </a>
  );
}
