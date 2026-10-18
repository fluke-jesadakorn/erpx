"use client";
import { createContext, useContext, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/foundation";
import { contextIsSwitching, registerUnsavedWork, setContextSwitching } from "@/lib/work_projection/context-switch";

import { usePathname, useSearchParams } from "next/navigation";
const DraftScope = createContext<{ scope: string; close: (action: () => void, root?: () => Element | null) => void } | undefined>(undefined);
export const useFinanceDraftScope = () => useContext(DraftScope)?.scope;
export function useFinanceDraftAction() {
  const context=useContext(DraftScope);
  return (action:()=>void)=>context?context.close(action):action();
}
export function useFinanceDraftClose(action: () => void, options?: { root: () => Element | null }) {
  const context = useContext(DraftScope);
  return () => context ? context.close(action, options?.root) : action();
}
export function captureFinanceForm() {
  const form = document.activeElement?.closest<HTMLFormElement>("form[data-finance-draft]");
  return form ? { form, formEditVersion: form.getAttribute("data-erpx-edit-version") } : undefined;
}
export function acknowledgeFinanceForm(completion: ReturnType<typeof captureFinanceForm>) {
  if (completion) {
    window.dispatchEvent(new CustomEvent("erpx:finance-command-acknowledged", { detail: completion }));
    window.dispatchEvent(new CustomEvent("erpx:record-command-saved", { detail: {...completion,financeAcknowledged:true} }));
  }
}

/** Track only marked mutation forms. Searches and filters are never drafts. */
export function FinanceDraftBoundary({ children }: { children: ReactNode }) {
  const scope = useId(), trigger = useRef<HTMLAnchorElement | null>(null), dirtyForms=useRef(new Set<HTMLFormElement>());
  const [destination, setDestination] = useState<string | null>(null), [closing, setClosing] = useState(false);
  const closeGuard = useRef<(action: () => void, root?: () => Element | null) => void>(action => action()), pendingClose = useRef<{action: () => void; root?: Element; readRoot?: () => Element | null} | null>(null), clearDrafts = useRef<(root?: Element) => void>(() => {});
  const pathname = usePathname(), query = useSearchParams().toString();
  useEffect(() => {
    // List filters/pagination can refresh this route while its drawer retains
    // the same DOM draft. Keep those connected forms across query changes.
    const forms = dirtyForms.current;
    const source = new URL(pathname + (query ? "?" + query : "") + window.location.hash, window.location.origin).href, historyState = window.history.state;
    const dirty = (root?: Element) => {
      for (const form of forms) if (!form.isConnected) forms.delete(form);
      return [...forms].some(form => !root || root.contains(form));
    };
    const unregister = registerUnsavedWork(dirty);
    clearDrafts.current = root => { for (const form of forms) if (!root || root.contains(form)) forms.delete(form); };
    closeGuard.current = (action, readRoot) => {
      const root = readRoot?.();
      if (readRoot && (!root || !root.isConnected)) return;
      if (!dirty(root ?? undefined)) return action();
      pendingClose.current = {action, root: root ?? undefined, readRoot}; setClosing(true);
    };
    const edited = (event: Event) => {
      const form = event.target instanceof Element ? event.target.closest<HTMLFormElement>("form[data-finance-draft]") : null;
      if (!form || form.getAttribute("data-finance-draft") !== scope) return;
      form.setAttribute("data-erpx-edit-version", crypto.randomUUID());
      forms.add(form);
    };
    const saved = (event: Event) => {
      const detail = (event as CustomEvent<{ form?: HTMLFormElement; formEditVersion?: string | null }>).detail;
      if (detail?.form && detail.formEditVersion === detail.form.getAttribute("data-erpx-edit-version")) forms.delete(detail.form);
    };
    const discarded = (event: Event) => {
      const discardedRoot = (event as CustomEvent<{root?: Element}>).detail?.root;
      if (discardedRoot) for (const form of forms) if (discardedRoot.contains(form)) forms.delete(form);
    };
    const click = (event: MouseEvent) => {
      if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || !dirty()) return;
      const anchor = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>("a[href]") : null;
      if (!anchor || anchor.hasAttribute("download") || anchor.target && anchor.target !== "_self") return;
      const url = new URL(anchor.href);
      if (!["https:", "http:"].includes(url.protocol) || url.href === window.location.href || url.origin === window.location.origin && url.pathname === window.location.pathname && url.search === window.location.search) return;
      event.preventDefault(); event.stopImmediatePropagation();
      trigger.current = anchor; setDestination(url.href);
    };
    const unload = (event: BeforeUnloadEvent) => { if (dirty() && !contextIsSwitching()) event.preventDefault(); };
    const navigation = (window as Window & { navigation?: EventTarget }).navigation;
    const traverse = (event: Event) => {
      const next = event as Event & { navigationType?: string; destination?: { url: string } };
      if (next.navigationType === "traverse" && next.cancelable && next.destination?.url !== source && dirty()) {
        next.preventDefault(); setDestination(next.destination?.url ?? null);
      }
    };
    const back = (event: PopStateEvent) => {
      if (window.location.href !== source && dirty()) {
        event.stopImmediatePropagation();
        const target = window.location.href;
        window.history.pushState(historyState, "", source); setDestination(target);
      }
    };
    document.addEventListener("click", click, true);
    document.addEventListener("input", edited); document.addEventListener("change", edited);
    window.addEventListener("erpx:finance-command-acknowledged", saved);
    window.addEventListener("erpx:record-saved", discarded);
    window.addEventListener("beforeunload", unload); window.addEventListener("popstate", back, true);
    navigation?.addEventListener("navigate", traverse);
    return () => {
      unregister(); document.removeEventListener("click", click, true);
      document.removeEventListener("input", edited); document.removeEventListener("change", edited);
      window.removeEventListener("erpx:finance-command-acknowledged", saved);
      window.removeEventListener("erpx:record-saved", discarded);
      window.removeEventListener("beforeunload", unload); window.removeEventListener("popstate", back, true);
      navigation?.removeEventListener("navigate", traverse);
    };
  }, [scope, pathname, query]);
  return <DraftScope.Provider value={{scope, close: (action, root) => closeGuard.current(action, root)}}>{children}
    <Dialog appearance="standard" open={destination !== null || closing} title={closing ? "Discard unsaved Finance changes?" : "Leave unsaved Finance work?"} onClose={() => {setDestination(null); setClosing(false); pendingClose.current = null;}} finalFocus={() => trigger.current}>
      <p>Your changes have not been saved. Leaving will discard them.</p>
      <div className="ui-form-actions">
        <Button appearance="standard" data-autofocus onClick={() => {setDestination(null); setClosing(false); pendingClose.current = null;}}>Keep editing</Button>
        <Button appearance="standard" variant="danger" onClick={() => {
          if (closing) {
            const pending = pendingClose.current; pendingClose.current = null; setClosing(false);
            if (!pending || pending.readRoot && (!pending.root?.isConnected || pending.readRoot() !== pending.root)) return;
            clearDrafts.current(pending.root); pending.action(); return;
          }
          if (!destination) return;
          setContextSwitching(true); window.location.assign(destination);
        }}>{closing ? "Discard changes" : "Discard and leave"}</Button>
      </div>
    </Dialog>
  </DraftScope.Provider>;
}
