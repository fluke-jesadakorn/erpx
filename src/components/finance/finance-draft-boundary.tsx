"use client";
import { createContext, useContext, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/foundation";
import { contextIsSwitching, registerContextSwitchBlocker, registerUnsavedWork, setContextSwitching } from "@/lib/work_projection/context-switch";

import { usePathname, useSearchParams } from "next/navigation";
type PendingCommand = { form?: HTMLFormElement; returnHref?: string };
const DraftScope = createContext<{ scope: string; close: (action: () => void, root?: () => Element | null) => void; trackCommandPending: (token: object | string, form: HTMLFormElement | undefined, pending: boolean, returnHref?: string) => void } | undefined>(undefined);
export const useFinanceDraftScope = () => useContext(DraftScope)?.scope;
export function useFinanceDraftCommandPending() {
  const context = useContext(DraftScope);
  return (token: object | string, completion: ReturnType<typeof captureFinanceForm>, pending: boolean) => {
    context?.trackCommandPending(token, completion?.form, pending, pending && typeof window !== "undefined" ? window.location.href : undefined);
  };
}
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

/** Mark state-managed Finance edits through the same dirty-form boundary as native field events. */
export function markFinanceFormEdited(form: HTMLFormElement | null | undefined) {
  form?.dispatchEvent(new Event("input", { bubbles: true }));
}

/** Track dirty Finance forms and unresolved commands; searches and filters are never drafts. */
export function FinanceDraftBoundary({ children }: { children: ReactNode }) {
  const scope = useId(), trigger = useRef<HTMLAnchorElement | null>(null), dirtyForms=useRef(new Set<HTMLFormElement>()), pendingCommands=useRef(new Map<object|string,PendingCommand>());
  const [destination, setDestination] = useState<string | null>(null), [closing, setClosing] = useState(false), [commandPending,setCommandPending]=useState(false);
  const closeGuard = useRef<(action: () => void, root?: () => Element | null) => void>(action => action()), pendingClose = useRef<{action: () => void; root?: Element; readRoot?: () => Element | null} | null>(null), clearDrafts = useRef<(root?: Element) => void>(() => {});
  const pathname = usePathname(), query = useSearchParams().toString();
  useEffect(() => {
    // List filters/pagination can refresh this route while its drawer retains
    // the same DOM draft. Keep those connected forms across query changes.
    const forms = dirtyForms.current;
    const source = new URL(pathname + (query ? "?" + query : "") + window.location.hash, window.location.origin).href, historyState = window.history.state;
    const discardDisconnected = (forms:Set<HTMLFormElement>) => { for (const form of forms) if (!form.isConnected) forms.delete(form); };
    const dirty = (root?: Element) => {
      discardDisconnected(forms);
      return [...forms].some(form => !root || root.contains(form))||[...pendingCommands.current.values()].some(command=>!command.form||!root||root.contains(command.form));
    };
    const unregister = registerUnsavedWork(dirty), unregisterSwitchBlocker = registerContextSwitchBlocker(() => pendingCommands.current.size > 0 ? "An original Finance request is awaiting confirmation. Return to Finance and retry it before switching account, Company, or role." : null);
    clearDrafts.current = root => { for (const form of forms) if (!root || root.contains(form)) forms.delete(form);for(const [token,command]of pendingCommands.current)if(command.form&&(!root||root.contains(command.form)))pendingCommands.current.delete(token); };
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
      unregister(); unregisterSwitchBlocker(); document.removeEventListener("click", click, true);
      document.removeEventListener("input", edited); document.removeEventListener("change", edited);
      window.removeEventListener("erpx:finance-command-acknowledged", saved);
      window.removeEventListener("erpx:record-saved", discarded);
      window.removeEventListener("beforeunload", unload); window.removeEventListener("popstate", back, true);
      navigation?.removeEventListener("navigate", traverse);
    };
  }, [scope, pathname, query]);
  const stayHere=()=>{setDestination(null);setClosing(false);pendingClose.current=null;};
  const trackCommandPending = (token: object|string, form: HTMLFormElement | undefined, pending: boolean, returnHref?: string) => {
    if (pending) {
      const existing=pendingCommands.current.get(token);
      pendingCommands.current.set(token, {form,returnHref:existing?.returnHref??returnHref});
      setCommandPending(pendingCommands.current.size > 0);
      return;
    }
    pendingCommands.current.delete(token);
    setCommandPending(pendingCommands.current.size > 0);
  };
  const returnToOriginalRequest=()=>{
    const href=[...pendingCommands.current.values()].map(command=>command.returnHref).find((value):value is string=>typeof value==="string");
    if(!href||typeof window==="undefined")return stayHere();
    try{
      const target=new URL(href,window.location.href);
      if(target.origin!==window.location.origin||!["http:","https:"].includes(target.protocol)||target.href===window.location.href)return stayHere();
      setContextSwitching(true);window.location.assign(target.href);
    }catch{return stayHere();}
  };
  return <DraftScope.Provider value={{scope, close: (action, root) => closeGuard.current(action, root),trackCommandPending}}>{children}
    <Dialog appearance="standard" open={destination !== null || closing} title={commandPending?"Confirm pending Finance action":closing ? "Discard unsaved Finance changes?" : "Leave unsaved Finance work?"} onClose={stayHere} finalFocus={() => trigger.current}>
      <p>{commandPending?"The original Finance request may have completed. Retry its original confirmation before leaving this page.":"Your changes have not been saved. Leaving will discard them."}</p>
      <div className="ui-form-actions">
        {commandPending?<Button appearance="standard" data-autofocus onClick={returnToOriginalRequest}>Return to the original request</Button>:<Button appearance="standard" data-autofocus onClick={stayHere}>Keep editing</Button>}
        {!commandPending&&<Button appearance="standard" variant="danger" onClick={() => {
          if (closing) {
            const pending = pendingClose.current; pendingClose.current = null; setClosing(false);
            if (!pending || pending.readRoot && (!pending.root?.isConnected || pending.readRoot() !== pending.root)) return;
            clearDrafts.current(pending.root); pending.action(); return;
          }
          if (!destination) return;
          setContextSwitching(true); window.location.assign(destination);
        }}>{closing ? "Discard changes" : "Discard and leave"}</Button>}
      </div>
    </Dialog>
  </DraftScope.Provider>;
}
