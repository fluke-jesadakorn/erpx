"use client";

import { Button as ButtonPrimitive } from "@base-ui/react/button";
import { LoaderCircle } from "lucide-react";
import { forwardRef, useSyncExternalStore, type ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/ui";
import { buttonVariants, type UiButtonVariant, type UiSize } from "./button-variants";

import type { UiAppearance } from "./appearance";

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: UiButtonVariant;
  appearance?: UiAppearance;
  size?: UiSize | "layout";
  pending?: boolean;
  /** Forms with a client onSubmit opt in; native/server forms stay usable. */
  clientManaged?: boolean;
};
const subscribeHydration = () => () => {};
const clientHydrated = () => true;
const serverHydrated = () => false;

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    className,
    appearance = "legacy",
    variant = "primary",
    size = "default",
    pending = false,
    clientManaged = false,
    disabled,
    type = "button",
    children,
    ...props
  },
  ref,
) {
  const effectiveVariant = appearance === "standard" && variant === "plain" ? "ghost" : variant;
  const managedSubmit = clientManaged && type === "submit";
  const hydrated = useSyncExternalStore(
    subscribeHydration,
    clientHydrated,
    managedSubmit ? serverHydrated : clientHydrated,
  );
  return (
    <ButtonPrimitive
      ref={ref}
      type={type}
      data-slot="button"
      data-size={size}
      data-variant={effectiveVariant}
      data-appearance={appearance}
      data-client-managed-ready={managedSubmit ? String(hydrated) : undefined}
      className={cn(buttonVariants({ variant: effectiveVariant, size }), appearance === "standard" && "ui-standard-control", className)}
      disabled={disabled || pending || (managedSubmit && !hydrated)}
      aria-busy={pending || undefined}
      {...props}
    >
      {pending && (
        <LoaderCircle
          className="animate-spin motion-reduce:animate-none"
          size={16}
          aria-hidden="true"
        />
      )}
      {children}
    </ButtonPrimitive>
  );
});
