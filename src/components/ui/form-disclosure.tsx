"use client";

import { Children, isValidElement, useEffect, useRef, useState, useSyncExternalStore, type ComponentPropsWithRef, type ReactNode } from "react";
import type { UiAppearance } from "./appearance";
import { Dialog } from "./dialog";
import { DraftFormDrawer } from "./draft-form-drawer";
import { cn } from "@/lib/shared/ui";
import { registerUnsavedWork } from "@/lib/work_projection/context-switch";
import { InlineNotice } from "./foundation";

function labelText(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (isValidElement<{ children?: ReactNode }>(node)) return labelText(node.props.children);
  return Children.toArray(node).map(labelText).join("");
}

const subscribeHydration = () => () => {};
const clientHydrated = () => true;
const serverHydrated = () => false;

/** Long forms launched from the current page use a drawer; route editors stay inline. */
export function FormDisclosure({ children, open, onToggle, finalFocus, className, inline = false, drawerTitle, contentClassName, dismissDisabled, onDiscard, error, appearance = "legacy", ...props }: ComponentPropsWithRef<"details"> & {
  finalFocus?: () => HTMLElement | null;
  inline?: boolean;
  appearance?: UiAppearance;
  drawerTitle?: string;
  contentClassName?: string;
  dismissDisabled?: boolean;
  /** Controlled editors supply their own reset; closing alone must not claim to discard their values. */
  onDiscard?: () => void;
  /** Command failures stay visible inside the open editor. */
  error?: string;
}) {
  const hydrated = useSyncExternalStore(subscribeHydration, clientHydrated, serverHydrated);
  const [dirty, setDirty] = useState(false);
  useEffect(() => registerUnsavedWork(() => dirty), [dirty]);
  const bodyRef = useRef<HTMLDivElement>(null);
  const editedForms = useRef(new Set<HTMLFormElement>());
  const nonFormEdits = useRef(false);
  useEffect(() => {
    function saved(event: Event) {
      const detail = (event as CustomEvent<{form?:HTMLFormElement;formEditVersion?:string|null}>).detail;
      const form = detail?.form;
      if (!form || !bodyRef.current?.contains(form) || !detail.formEditVersion ||
        detail.formEditVersion !== form.getAttribute("data-erpx-edit-version")) return;
      editedForms.current.delete(form);
      setDirty(nonFormEdits.current || [...editedForms.current].some(item => bodyRef.current?.contains(item)));
    }
    window.addEventListener("erpx:record-command-saved", saved);
    return () => window.removeEventListener("erpx:record-command-saved", saved);
  }, []);
  const [expanded, setExpanded] = useState(open ?? false);
  const [previousOpen, setPreviousOpen] = useState(open);
  if (previousOpen !== open) {
    setPreviousOpen(open);
    setExpanded(open ?? false);
  }
  if (inline) return <details {...props} className={className} open={open} onToggle={onToggle}>{children}</details>;
  const nodes = Children.toArray(children);
  const summary = nodes.find(node => isValidElement(node) && node.type === "summary");
  const content = nodes.filter(node => node !== summary);
  const body = <div className={contentClassName}><div ref={bodyRef} onChangeCapture={event => {
    const form = (event.target as HTMLElement).closest("form");
    if (form) { form.setAttribute("data-erpx-edit-version", crypto.randomUUID()); editedForms.current.add(form); }
    else nonFormEdits.current = true;
    setDirty(true);
  }} className={cn("form-disclosure-content", className)}>{error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{content}</div></div>;
  const title = drawerTitle || labelText(summary).trim() || "Edit details";
  return <>
    <details {...props} inert={props.inert || !hydrated} data-client-managed-ready={String(hydrated)} className={className} open={expanded} onToggle={event => {
      setExpanded(event.currentTarget.open);
      onToggle?.(event);
    }}>{summary}</details>
    {appearance === "standard" ? <DraftFormDrawer retainFormContents={dirty} finalFocus={finalFocus} appearance={appearance} open={expanded} dirty={dirty} onClose={() => setExpanded(false)} onDiscard={() => {if(onDiscard){onDiscard();editedForms.current.clear();nonFormEdits.current=false;setDirty(false);}setExpanded(false);}} confirmationTitle={onDiscard ? "Discard changes?" : "Unsaved changes"} confirmationDescription={onDiscard ? "Your draft has unsaved changes." : "Your changes have not been saved. Closing this editor does not submit them."} discardLabel={onDiscard ? "Discard draft" : "Close without saving"} title={title} dismissDisabled={dismissDisabled}>{body}</DraftFormDrawer> : <Dialog retainFormContents={dirty} finalFocus={finalFocus} appearance={appearance} open={expanded} onClose={() => setExpanded(false)} title={title} presentation="form-drawer" dismissDisabled={dismissDisabled}>{body}</Dialog>}
  </>;
}
