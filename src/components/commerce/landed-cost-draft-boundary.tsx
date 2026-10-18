"use client";
import { Children, isValidElement, createContext, useContext, useEffect, useId, useRef, useState, type ComponentPropsWithRef, type ReactNode } from "react";
import { Dialog } from "@/components/ui/dialog";
import { DraftFormDrawer } from "@/components/ui/draft-form-drawer";
import { Button } from "@/components/ui/foundation";
import { contextIsSwitching, registerUnsavedWork, setContextSwitching } from "@/lib/work_projection/context-switch";
import { usePathname, useSearchParams } from "next/navigation";
const DraftScope = createContext<string | undefined>(undefined);

export function LandedCostForm({onInputCapture, onChangeCapture, onClickCapture, children, ...props}: ComponentPropsWithRef<"form">) {
  const scope = useContext(DraftScope) ?? "landedCost-editor";
  const edited = (event: React.SyntheticEvent<HTMLFormElement>) => event.currentTarget.setAttribute("data-erpx-edit-version", crypto.randomUUID());
  return <form {...props} data-slot="form-layout" data-layout="grid" data-landed-cost-draft={scope} onInputCapture={event => {edited(event); onInputCapture?.(event);}} onChangeCapture={event => {edited(event); onChangeCapture?.(event);}} onClickCapture={event => {if (event.target instanceof Element && event.target.closest('button[type="button"]')) edited(event); onClickCapture?.(event);}}>{Children.toArray(children)}</form>;
}

export function LandedCostFormDrawer({children, inline = false, ...props}: Omit<ComponentPropsWithRef<typeof DraftFormDrawer>, "dirty"> & {inline?: boolean}) {
  const root = useRef<HTMLDivElement>(null), dirtyForms = useRef(new Set<HTMLFormElement>());
  const [dirty, setDirty] = useState(false);
  useEffect(() => {
    const unregister = registerUnsavedWork(() => [...dirtyForms.current].some(form => form.isConnected));
    const saved = (event: Event) => {
      const detail = (event as CustomEvent<{form?: HTMLFormElement; formEditVersion?: string | null; landedCostAcknowledged?: boolean}>).detail;
      if (detail?.landedCostAcknowledged && detail.form && detail.formEditVersion === detail.form.getAttribute("data-erpx-edit-version")) {
        dirtyForms.current.delete(detail.form); setDirty([...dirtyForms.current].some(form => form.isConnected));
      }
    };
    window.addEventListener("erpx:record-command-saved", saved);
    return () => {unregister(); window.removeEventListener("erpx:record-command-saved", saved);};
  }, []);
  if (inline) return <>{children}</>;
  return <DraftFormDrawer {...props} dirty={dirty} onDiscard={() => {
    window.dispatchEvent(new CustomEvent("erpx:record-saved", {detail: {root: root.current}}));
    dirtyForms.current.clear(); setDirty(false); (props.onDiscard ?? props.onClose)();
  }}><div ref={root} data-work-dirty={dirty ? "true" : undefined} onClickCapture={event => {
    const form = event.target instanceof Element && event.target.closest('button[type="button"]')?.closest("form");
    if (form) {dirtyForms.current.add(form); setDirty(true);}
  }} onInputCapture={event => {
    const form = (event.target as HTMLElement).closest("form");
    if (form) {dirtyForms.current.add(form); setDirty(true);}
  }} onChangeCapture={event => {
    const form = (event.target as HTMLElement).closest("form");
    if (form) {dirtyForms.current.add(form); setDirty(true);}
  }}>{children}</div></DraftFormDrawer>;
}

/** LandedCost editors share captured form acknowledgements and explicit resets. */
export function LandedCostDisclosure({children, drawerTitle, open, initialOpen = false, inline = false, onBeforeOpen, onOpenChange, onDiscard, error, dismissDisabled}: {
  children: ReactNode; appearance?: "standard"; drawerTitle: string; open?: boolean; initialOpen?: boolean; inline?: boolean; onBeforeOpen?: () => void;
  onOpenChange?: (open: boolean) => void; onDiscard: () => void; error?: string; dismissDisabled?: boolean;
}) {
  const [expanded, setExpanded] = useState(initialOpen);
  const nodes = Children.toArray(children), summary = nodes.find(node => isValidElement(node) && node.type === "summary");
  const label = isValidElement<{children?: ReactNode}>(summary) ? summary.props.children : drawerTitle;
  const change = (value: boolean) => {if(value)onBeforeOpen?.();setExpanded(value); onOpenChange?.(value);};
  if (inline) return <div>{nodes.filter(node => node !== summary)}</div>;
  return <><Button appearance="standard" type="button" variant="secondary" onClick={() => change(true)}>{label}</Button>
    <LandedCostFormDrawer appearance="standard" open={open ?? expanded} title={drawerTitle} onClose={() => change(false)} onDiscard={() => {onDiscard(); change(false);}} dismissDisabled={dismissDisabled}>
      {error && <p role="alert">{error}</p>}{nodes.filter(node => node !== summary)}
    </LandedCostFormDrawer></>;
}

/** Only marked mutation forms participate; route filters remain ordinary UI.
 * Matching edit-version acknowledgement preserves fields typed during a save. */
export function LandedCostDraftBoundary({children}: {children: ReactNode}) {
  const scope = useId(), forms = useRef(new Set<HTMLFormElement>()), trigger = useRef<HTMLAnchorElement | null>(null);
  const [destination, setDestination] = useState<string | null>(null);
  const pathname = usePathname(), query = useSearchParams().toString();
  useEffect(() => {
    const current = forms.current, source = window.location.href, historyState = window.history.state;
    const dirty = () => {for (const form of current) if (!form.isConnected) current.delete(form); return current.size > 0;};
    const unregister = registerUnsavedWork(dirty);
    const edited = (event: Event) => {
      const form = event.target instanceof Element ? event.target.closest<HTMLFormElement>("form[data-landed-cost-draft]") : null;
      if (!form || form.getAttribute("data-landed-cost-draft") !== scope) return;
      current.add(form);
    };
    const saved = (event: Event) => {
      const detail = (event as CustomEvent<{form?: HTMLFormElement; formEditVersion?: string | null; landedCostAcknowledged?: boolean}>).detail;
      if (detail?.landedCostAcknowledged && detail.form && detail.formEditVersion === detail.form.getAttribute("data-erpx-edit-version")) current.delete(detail.form);
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
    const buttonEdit = (event: MouseEvent) => {if (event.target instanceof Element && event.target.closest('button[type="button"]')) edited(event);};
    document.addEventListener("input", edited); document.addEventListener("change", edited); document.addEventListener("click", buttonEdit); document.addEventListener("click", click, true);
    window.addEventListener("erpx:record-command-saved", saved); window.addEventListener("erpx:record-saved", discarded);
    window.addEventListener("beforeunload", unload); window.addEventListener("popstate", back, true); navigation?.addEventListener("navigate", traverse);
    return () => {
      unregister(); document.removeEventListener("input", edited); document.removeEventListener("change", edited); document.removeEventListener("click", buttonEdit); document.removeEventListener("click", click, true);
      window.removeEventListener("erpx:record-command-saved", saved); window.removeEventListener("erpx:record-saved", discarded);
      window.removeEventListener("beforeunload", unload); window.removeEventListener("popstate", back, true); navigation?.removeEventListener("navigate", traverse);
    };
  }, [scope, pathname, query]);
  return <DraftScope.Provider value={scope}>{children}<Dialog appearance="standard" open={destination !== null} title="Leave unsaved landed cost work?" onClose={() => setDestination(null)} finalFocus={() => trigger.current}>
    <p>Your changes have not been saved. Leaving will discard this local draft.</p><div className="ui-form-actions">
      <Button appearance="standard" data-autofocus onClick={() => setDestination(null)}>Keep editing</Button>
      <Button appearance="standard" variant="danger" onClick={() => {if (destination) {setContextSwitching(true); window.location.assign(destination);}}}>Discard and leave</Button>
    </div></Dialog></DraftScope.Provider>;
}
