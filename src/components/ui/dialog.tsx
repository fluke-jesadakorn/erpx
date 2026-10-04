"use client";

import { Dialog as DialogPrimitive } from "@base-ui/react/dialog";
import { X } from "lucide-react";
import { useRef, type ReactNode } from "react";
import type { UiAppearance } from "./appearance";
import { cn } from "@/lib/ui";
import { buttonVariants } from "./button-variants";
import "./dialog.css";

export const DialogClose = DialogPrimitive.Close;

export function FormDrawer({ inline = false, ...props }: {
  inline?: boolean;
  appearance?: UiAppearance;
  open: boolean;
  onClose: () => void;
  title: string;
  dismissDisabled?: boolean;
  children: ReactNode;
}) {
  return inline ? <>{props.children}</> : <Dialog {...props} presentation="form-drawer" />;
}

export function Dialog({
  id,
  open,
  onClose,
  title,
  children,
  presentation = "default",
  appearance = "legacy",
  className,
  bodyClassName,
  dismissDisabled = false,
  closeLabel = "Close dialog",
  showHeader = true,
  "data-testid": testId,
}: {
  id?: string;
  appearance?: UiAppearance;
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  presentation?: "default" | "responsive-panel" | "fullscreen" | "people-drawer" | "form-drawer";
  className?: string;
  bodyClassName?: string;
  dismissDisabled?: boolean;
  closeLabel?: string;
  showHeader?: boolean;
  "data-testid"?: string;
}) {
  const popup = useRef<HTMLDivElement>(null);
  const panel = presentation !== "default";
  const positions = {
    default:
      "relative m-auto max-h-[calc(100dvh-3rem)] w-[min(620px,calc(100%-2rem))] rounded-[var(--ui-radius-xl)]",
    "responsive-panel":
      "fixed inset-x-0 bottom-0 max-h-[90dvh] w-full rounded-t-[var(--ui-radius-xl)] lg:inset-y-0 lg:right-0 lg:left-auto lg:max-h-dvh lg:w-[min(440px,100%)] lg:rounded-none lg:rounded-l-[var(--ui-radius-xl)]",
    "people-drawer":
      "fixed inset-x-0 bottom-0 h-[min(92dvh,760px)] max-h-[92dvh] w-full rounded-t-[var(--ui-radius-xl)] sm:inset-y-0 sm:right-0 sm:left-auto sm:h-dvh sm:max-h-dvh sm:w-[min(440px,100%)] sm:rounded-none sm:rounded-l-[var(--ui-radius-xl)]",
    fullscreen: "fixed inset-0 h-dvh max-h-dvh w-full border-0",
    "form-drawer": "fixed inset-y-0 right-0 h-dvh w-full sm:w-[min(800px,100%)]",
  };
  return (
    <DialogPrimitive.Root
      open={open}
      disablePointerDismissal={dismissDisabled}
      onOpenChange={(next, details) => {
        if (!next) {
          // The caller owns dirty-draft confirmation. Keep this controlled dialog
          // mounted if onClose declines dismissal or opens a confirmation dialog.
          details.cancel();
          if (!dismissDisabled) onClose();
        }
      }}
    >
      <DialogPrimitive.Portal>
        <DialogPrimitive.Backdrop
          data-slot="dialog-backdrop"
          className="fixed inset-0 z-[80] bg-slate-950/60"
        />
        <DialogPrimitive.Viewport className="pointer-events-none fixed inset-0 z-[80] flex overflow-y-auto">
          <DialogPrimitive.Popup
            id={id}
            ref={popup}
            data-slot="dialog"
            data-presentation={presentation}
            data-appearance={appearance}
            data-testid={testId}
            initialFocus={() =>
              popup.current?.querySelector<HTMLElement>("[data-autofocus]") ??
              (presentation === "form-drawer"
                ? popup.current?.querySelector<HTMLElement>(
                    'input:not([type="hidden"]):not([disabled]), select:not([disabled]), textarea:not([disabled])',
                  )
                : null) ?? true
            }
            className={cn(
              "pointer-events-auto flex min-h-0 flex-col border border-[var(--ui-line)] bg-[var(--ui-card)] text-[var(--ui-ink)] shadow-lg outline-none",
              positions[presentation],
              appearance === "standard" && "ui-standard-dialog",
              className,
            )}
          >
            {showHeader ? (
              <header className="flex shrink-0 items-center justify-between gap-3 border-b border-[var(--ui-line)] px-5 py-3">
                <DialogPrimitive.Title className="m-0 min-w-0 text-lg font-semibold wrap-anywhere">
                  {title}
                </DialogPrimitive.Title>
                <DialogPrimitive.Close
                  aria-label={closeLabel}
                  title={closeLabel}
                  disabled={dismissDisabled}
                  className={cn(buttonVariants({ variant: "ghost" }), "size-11 shrink-0 p-0")}
                >
                  <X size={18} aria-hidden="true" />
                </DialogPrimitive.Close>
              </header>
            ) : (
              <DialogPrimitive.Title className="sr-only">{title}</DialogPrimitive.Title>
            )}
            <div
              data-slot="dialog-body"
              className={cn(
                "min-h-0 overflow-y-auto",
                panel ? "flex-1" : "p-5",
                presentation === "responsive-panel" &&
                  "p-5 pb-[calc(1.25rem+env(safe-area-inset-bottom))]",
                presentation === "form-drawer" &&
                  "flex-1 p-5 pb-[calc(1.25rem+env(safe-area-inset-bottom))]",
                (presentation === "fullscreen" || presentation === "people-drawer") &&
                  "flex overflow-hidden p-0 pb-[env(safe-area-inset-bottom)]",
                bodyClassName,
              )}
            >
              {open && children}
            </div>
          </DialogPrimitive.Popup>
        </DialogPrimitive.Viewport>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
