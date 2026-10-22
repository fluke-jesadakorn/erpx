"use client";
import { useEffect, useRef, useState, type ComponentPropsWithRef } from "react";
import { DraftFormDrawer } from "@erpx/ui/components/draft-form-drawer";
import { registerUnsavedWork } from "@erpx/feature-work_projection/lib/context-switch";

export function PayrollFormDrawer({children, ...props}: Omit<ComponentPropsWithRef<typeof DraftFormDrawer>, "dirty">) {
  const root = useRef<HTMLDivElement>(null), dirtyForms = useRef(new Set<HTMLFormElement>());
  const [dirty, setDirty] = useState(false);
  useEffect(() => {
    const unregister = registerUnsavedWork(() => [...dirtyForms.current].some(form => form.isConnected));
    const saved = (event: Event) => {
      const detail = (event as CustomEvent<{form?: HTMLFormElement; formEditVersion?: string | null; payrollAcknowledged?: boolean}>).detail;
      if (detail?.payrollAcknowledged && detail.form && detail.formEditVersion === detail.form.getAttribute("data-erpx-edit-version")) {
        dirtyForms.current.delete(detail.form); setDirty([...dirtyForms.current].some(form => form.isConnected));
      }
    };
    window.addEventListener("erpx:record-command-saved", saved);
    return () => {unregister(); window.removeEventListener("erpx:record-command-saved", saved);};
  }, []);
  return <DraftFormDrawer {...props} dirty={dirty} onDiscard={() => {
    window.dispatchEvent(new CustomEvent("erpx:record-saved", {detail: {root: root.current}}));
    dirtyForms.current.clear(); setDirty(false); (props.onDiscard ?? props.onClose)();
  }}><div ref={root} data-work-dirty={dirty ? "true" : undefined} onInputCapture={event => {
    const form = (event.target as HTMLElement).closest("form");
    if (form) {dirtyForms.current.add(form); setDirty(true);}
  }} onChangeCapture={event => {
    const form = (event.target as HTMLElement).closest("form");
    if (form) {dirtyForms.current.add(form); setDirty(true);}
  }}>{children}</div></DraftFormDrawer>;
}

