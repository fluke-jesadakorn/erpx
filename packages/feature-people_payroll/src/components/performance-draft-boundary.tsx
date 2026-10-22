"use client";
import { createContext, useContext, useEffect, useId, useRef, useState, type ComponentPropsWithRef, type ReactNode } from "react";
import { Dialog } from "@erpx/ui/components/dialog";
import { DraftFormDrawer } from "@erpx/ui/components/draft-form-drawer";
import { Button } from "@erpx/ui/components/foundation";
import { contextIsSwitching, registerUnsavedWork, setContextSwitching } from "@erpx/feature-work_projection/lib/context-switch";
import { usePathname, useSearchParams } from "next/navigation";
const DraftScope = createContext<string | undefined>(undefined);

export function PerformanceForm({onInputCapture, onChangeCapture, onClickCapture, ...props}: ComponentPropsWithRef<"form">) {
  const scope = useContext(DraftScope) ?? "performance-editor";
  const edited = (event: React.SyntheticEvent<HTMLFormElement>) => event.currentTarget.setAttribute("data-erpx-edit-version", crypto.randomUUID());
  return <form {...props} data-slot="form-layout" data-layout="grid" data-performance-draft={scope} onInputCapture={event => {edited(event); onInputCapture?.(event);}} onChangeCapture={event => {edited(event); onChangeCapture?.(event);}} onClickCapture={event => {if (event.target instanceof Element && event.target.closest("button[type=button]")) {edited(event); event.currentTarget.dispatchEvent(new Event("change", {bubbles:true}));} onClickCapture?.(event);}} />;
}

export function PerformanceFormDrawer({children, ...props}: Omit<ComponentPropsWithRef<typeof DraftFormDrawer>, "dirty">) {
  const root = useRef<HTMLDivElement>(null), dirtyForms = useRef(new Set<HTMLFormElement>());
  const [dirty, setDirty] = useState(false);
  useEffect(() => {
    const unregister = registerUnsavedWork(() => [...dirtyForms.current].some(form => form.isConnected));
    const saved = (event: Event) => {
      const detail = (event as CustomEvent<{form?: HTMLFormElement; formEditVersion?: string | null; performanceAcknowledged?: boolean}>).detail;
      if (detail?.performanceAcknowledged && detail.form && detail.formEditVersion === detail.form.getAttribute("data-erpx-edit-version")) {
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

/** Only marked mutation forms participate; route filters remain ordinary UI.
 * Matching edit-version acknowledgement preserves fields typed during a save. */
export function PerformanceDraftBoundary({children}: {children: ReactNode}) {
  const scope = useId(), forms = useRef(new Set<HTMLFormElement>()), trigger = useRef<HTMLAnchorElement | null>(null);
  const [destination, setDestination] = useState<string | null>(null);
  const pathname = usePathname(), query = useSearchParams().toString();
  useEffect(() => {
    const current = forms.current, source = window.location.href, historyState = window.history.state;
    const dirty = () => {for (const form of current) if (!form.isConnected) current.delete(form); return current.size > 0;};
    const unregister = registerUnsavedWork(dirty);
    const edited = (event: Event) => {
      const form = event.target instanceof Element ? event.target.closest<HTMLFormElement>("form[data-performance-draft]") : null;
      if (!form || form.getAttribute("data-performance-draft") !== scope) return;
      form.setAttribute("data-erpx-edit-version", crypto.randomUUID()); current.add(form);
    };
    const saved = (event: Event) => {
      const detail = (event as CustomEvent<{form?: HTMLFormElement; formEditVersion?: string | null; performanceAcknowledged?: boolean}>).detail;
      if (detail?.performanceAcknowledged && detail.form && detail.formEditVersion === detail.form.getAttribute("data-erpx-edit-version")) current.delete(detail.form);
    };
    const discarded = (event: Event) => {
      const root = (event as CustomEvent<{root?: Element}>).detail?.root;
      if (root) for (const form of current) if (root.contains(form)) current.delete(form);
    };
    const click = (event: MouseEvent) => {
      if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || !dirty()) return;
      const anchor = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>("a[href]") : null;
      if (!anchor || anchor.hasAttribute("download") || anchor.target && anchor.target !== "_self") return;
      const url = new URL(anchor.href);
      if (!["https:", "http:"].includes(url.protocol) || url.href === window.location.href || url.origin === window.location.origin && url.pathname === window.location.pathname && url.search === window.location.search) return;
      event.preventDefault(); event.stopImmediatePropagation(); trigger.current = anchor; setDestination(url.href);
    };
    const unload = (event: BeforeUnloadEvent) => {if (dirty() && !contextIsSwitching()) event.preventDefault();};
    const navigation = (window as Window & {navigation?: EventTarget}).navigation;
    const traverse = (event: Event) => {
      const next = event as Event & {navigationType?: string; destination?: {url: string}};
      if (next.navigationType === "traverse" && next.cancelable && next.destination?.url !== source && dirty()) {next.preventDefault(); setDestination(next.destination?.url ?? null);}
    };
    const back = (event: PopStateEvent) => {
      if (window.location.href !== source && dirty()) {event.stopImmediatePropagation(); const next = window.location.href; window.history.pushState(historyState, "", source); setDestination(next);}
    };
    document.addEventListener("input", edited); document.addEventListener("change", edited); document.addEventListener("click", click, true);
    window.addEventListener("erpx:record-command-saved", saved); window.addEventListener("erpx:record-saved", discarded);
    window.addEventListener("beforeunload", unload); window.addEventListener("popstate", back, true); navigation?.addEventListener("navigate", traverse);
    return () => {
      unregister(); document.removeEventListener("input", edited); document.removeEventListener("change", edited); document.removeEventListener("click", click, true);
      window.removeEventListener("erpx:record-command-saved", saved); window.removeEventListener("erpx:record-saved", discarded);
      window.removeEventListener("beforeunload", unload); window.removeEventListener("popstate", back, true); navigation?.removeEventListener("navigate", traverse);
    };
  }, [scope, pathname, query]);
  return <DraftScope.Provider value={scope}>{children}<Dialog appearance="standard" open={destination !== null} title="Leave unsaved Performance work?" onClose={() => setDestination(null)} finalFocus={() => trigger.current}>
    <p>Your changes have not been saved. Leaving will discard this local draft.</p><div className="ui-form-actions">
      <Button appearance="standard" data-autofocus onClick={() => setDestination(null)}>Keep editing</Button>
      <Button appearance="standard" variant="danger" onClick={() => {if (destination) {setContextSwitching(true); window.location.assign(destination);}}}>Discard and leave</Button>
    </div></Dialog></DraftScope.Provider>;
}
