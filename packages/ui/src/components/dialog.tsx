"use client";

import { Dialog as DialogPrimitive } from "@base-ui/react/dialog";
import { X } from "lucide-react";
import { useRef, useState, useEffect, type ReactNode } from "react";
import type { UiAppearance } from "@erpx/ui/components/appearance";
import { cn } from "@erpx/ui/lib/cn";
import { buttonVariants } from "@erpx/ui/components/button-variants";
import "@erpx/ui/components/dialog.css";
import { useEmbeddedRecord } from "@erpx/ui/components/record-context";
import {useWorkspaceEditor,useWorkspaceOverlay} from "@erpx/ui/components/workspace-overlay";
import { Button } from "@erpx/ui/components/button";

export const DialogClose = DialogPrimitive.Close;

export function FormDrawer({ inline = false, ...props }: {
  inline?: boolean;
  appearance?: UiAppearance;
  open: boolean;
  onClose: () => void;
  title: string;
  dismissDisabled?: boolean;
  retainFormContents?: boolean;
  children: ReactNode;
}) {
  return inline ? <>{props.children}</> : <Dialog {...props} presentation="form-drawer" />;
}

export function Dialog({
  id,
  workspaceRole = "editor",
  suspended = false,
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
  retainFormContents = false,
  finalFocus,
  "data-testid": testId,
}: {
  id?: string;
  workspaceRole?: "editor" | "detail";
  suspended?: boolean;
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
  /** Keep a visited form mounted; unopened tools still load only when opened. */
  retainFormContents?: boolean;
  finalFocus?: () => HTMLElement | null;
  "data-testid"?: string;
}) {
  const popup = useRef<HTMLDivElement>(null);
  const embedded = useEmbeddedRecord();
  const workspace = useWorkspaceOverlay();
  useWorkspaceEditor(open && presentation === "form-drawer" && workspaceRole === "editor");
  const [hasOpened, setHasOpened] = useState(retainFormContents && open);
  if (retainFormContents && open && !hasOpened) setHasOpened(true);
  const keepFormMounted = retainFormContents && (presentation === "form-drawer" || presentation === "fullscreen") && hasOpened;
  // Embedded editors retain their existing explicit-discard behavior.
  if (embedded && !workspace.active && presentation === "form-drawer") return open ? <EmbeddedForm title={title} onClose={onClose} dismissDisabled={dismissDisabled}>{children}</EmbeddedForm> : null;
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
      <DialogPrimitive.Portal keepMounted={keepFormMounted}>
        <DialogPrimitive.Backdrop
          hidden={suspended || !open}
          data-slot="dialog-backdrop"
          className="fixed inset-0 z-[80] bg-slate-950/60"
        />
        <DialogPrimitive.Viewport style={suspended || !open?{display:"none"}:undefined} hidden={suspended || !open} className="pointer-events-none fixed inset-0 z-[80] flex overflow-y-auto">
          <DialogPrimitive.Popup
            id={id}
            aria-hidden={suspended || !open || undefined}
            ref={popup}
            data-slot="dialog"
            data-presentation={presentation}
            data-appearance={appearance}
            data-testid={testId}
            finalFocus={finalFocus}
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
              {(open || keepFormMounted) && children}
            </div>
          </DialogPrimitive.Popup>
        </DialogPrimitive.Viewport>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

function EmbeddedForm({ title, onClose, children, dismissDisabled }: { title: string; onClose: () => void; children: ReactNode; dismissDisabled: boolean }) {
  const [dirty, setDirty] = useState(false), [confirming, setConfirming] = useState(false);
  const root=useRef<HTMLElement>(null);
  useEffect(() => { const saved = (event:Event) => {const form=(event as CustomEvent<{form?:Element|null}>).detail?.form;if(form&&root.current?.contains(form))setDirty(false);}; window.addEventListener("erpx:record-command-saved", saved); return () => window.removeEventListener("erpx:record-command-saved", saved); }, []);
  return <section ref={root} data-record-editor onChangeCapture={() => setDirty(true)}>
    <header className="ui-section-header"><h2>{title}</h2><Button appearance="standard" variant="secondary" disabled={dismissDisabled} onClick={() => dirty ? setConfirming(true) : onClose()}>Back to details</Button></header>
    {children}
    <Dialog appearance="standard" open={confirming} title="Discard unsaved changes?" onClose={() => setConfirming(false)}><p>Your changes have not been saved.</p><div className="ui-form-actions"><Button appearance="standard" onClick={() => setConfirming(false)}>Keep editing</Button><Button appearance="standard" variant="danger" onClick={() => {window.dispatchEvent(new CustomEvent("erpx:record-saved",{detail:{root:root.current}}));onClose();}}>Discard changes</Button></div></Dialog>
  </section>;
}
