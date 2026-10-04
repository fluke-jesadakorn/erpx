"use client";

import { Children, isValidElement, useState, type ComponentPropsWithRef, type ReactNode } from "react";
import type { UiAppearance } from "./appearance";
import { Dialog } from "./dialog";
import { DraftFormDrawer } from "./draft-form-drawer";
import { cn } from "@/lib/ui";

function labelText(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (isValidElement<{ children?: ReactNode }>(node)) return labelText(node.props.children);
  return Children.toArray(node).map(labelText).join("");
}

/** Long forms launched from the current page use a drawer; route editors stay inline. */
export function FormDisclosure({ children, open, onToggle, className, inline = false, drawerTitle, contentClassName, dismissDisabled, onDiscard, appearance = "legacy", ...props }: ComponentPropsWithRef<"details"> & {
  inline?: boolean;
  appearance?: UiAppearance;
  drawerTitle?: string;
  contentClassName?: string;
  dismissDisabled?: boolean;
  /** Controlled editors supply their own reset; closing alone must not claim to discard their values. */
  onDiscard?: () => void;
}) {
  const [dirty, setDirty] = useState(false);
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
  const body = <div className={contentClassName}><div onChangeCapture={appearance === "standard" ? () => setDirty(true) : undefined} className={cn("form-disclosure-content", className)}>{content}</div></div>;
  const title = drawerTitle || labelText(summary).trim() || "Edit details";
  return <>
    <details {...props} className={className} open={expanded} onToggle={event => {
      setExpanded(event.currentTarget.open);
      onToggle?.(event);
    }}>{summary}</details>
    {appearance === "standard" ? <DraftFormDrawer appearance={appearance} open={expanded} dirty={dirty} onClose={() => setExpanded(false)} onDiscard={() => {if(onDiscard){onDiscard();setDirty(false);}setExpanded(false);}} confirmationTitle={onDiscard ? "Discard changes?" : "Unsaved changes"} confirmationDescription={onDiscard ? "Your draft has unsaved changes." : "Your changes have not been saved. Closing this editor does not submit them."} discardLabel={onDiscard ? "Discard draft" : "Close without saving"} title={title} dismissDisabled={dismissDisabled}>{body}</DraftFormDrawer> : <Dialog appearance={appearance} open={expanded} onClose={() => setExpanded(false)} title={title} presentation="form-drawer" dismissDisabled={dismissDisabled}>{body}</Dialog>}
  </>;
}
