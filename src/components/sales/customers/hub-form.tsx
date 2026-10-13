"use client";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { DraftFormDrawer } from "@/components/ui/draft-form-drawer";
import { Dialog } from "@/components/ui/dialog";
import { useRouter } from "next/navigation";
import { registerUnsavedWork } from "@/lib/work_projection/context-switch";
import { Button } from "@/components/ui/foundation";
import styles from "@/components/sales/customers/hub.module.css";

type HistoryNavigationEvent = Event & { navigationType?: string; destination?: { url: string } };

export function HubForm({ title, label, children, initialOpen = false, primary = true }: { title: string; label: string; children: ReactNode; initialOpen?: boolean; primary?: boolean }) {
  const [open, setOpen] = useState(initialOpen), [dirty, setDirty] = useState(false), [generation, setGeneration] = useState(0);
  const body = useRef<HTMLDivElement>(null);
  const router = useRouter(), latestUrl = useRef(""), latestHistoryState = useRef<unknown>(null), [destination, setDestination] = useState<string | null>(null);
  const close = () => {
    setOpen(false);
    if (initialOpen) {
      const url = new URL(window.location.href);
      url.searchParams.delete("new"); url.searchParams.delete("edit");
      router.replace(`${url.pathname}${url.search}`, { scroll: false });
    }
  };
  useEffect(() => registerUnsavedWork(() => open && dirty), [open, dirty]);
  useEffect(() => {
    latestUrl.current = window.location.href;
    latestHistoryState.current = window.history.state;
    const saved = () => { setDirty(false); setOpen(false); setGeneration(value => value + 1); };
    const unload = (event: BeforeUnloadEvent) => { if (open && dirty) { event.preventDefault(); event.returnValue = ""; } };
    const back = (event: PopStateEvent) => {
      if (open && dirty && window.location.href !== latestUrl.current) {
        event.stopImmediatePropagation();
        const target = window.location.href;
        window.history.pushState(latestHistoryState.current, "", latestUrl.current);
        setDestination(target);
      }
    };
    // Cancel traversal before the browser changes the active route when supported.
    const navigation = (window as Window & { navigation?: EventTarget }).navigation;
    const traverse = (event: Event) => {
      const historyEvent = event as HistoryNavigationEvent;
      if (open && dirty && event.cancelable && historyEvent.navigationType === "traverse" && historyEvent.destination?.url !== latestUrl.current) {
        event.preventDefault();
        setDestination(historyEvent.destination?.url ?? null);
      }
    };
    navigation?.addEventListener("navigate", traverse);
    window.addEventListener("erpx:hub-saved", saved); window.addEventListener("beforeunload", unload); window.addEventListener("popstate", back, true);
    return () => { navigation?.removeEventListener("navigate", traverse); window.removeEventListener("erpx:hub-saved", saved); window.removeEventListener("beforeunload", unload); window.removeEventListener("popstate", back, true); };
  }, [dirty, open]);
  return <>
    <Button appearance="standard" variant={primary ? "primary" : "secondary"} type="button" onClick={() => setOpen(true)}>{label}</Button>
    <DraftFormDrawer title={title} open={open} dirty={dirty} onClose={close} onDiscard={() => { close(); setDirty(false); setGeneration(value => value + 1); }}>
      <div key={generation} ref={body} className={styles.formBody} data-work-dirty={open && dirty ? "true" : undefined} onChangeCapture={event => { if (!(event.target instanceof Element) || !event.target.closest('[data-work-dirty="false"]')) setDirty(true); }} onClickCapture={event => { const button = event.target instanceof Element ? event.target.closest('button[type="button"]') : null; if (button?.closest("form") && !button.closest('[data-work-dirty="false"]')) setDirty(true); }}>{children}</div>
    </DraftFormDrawer>
    <Dialog appearance="standard" open={!!destination} title="Discard changes?" onClose={() => setDestination(null)}><p>Your draft has unsaved changes.</p><div className="ui-form-actions"><Button appearance="standard" variant="secondary" onClick={() => setDestination(null)}>Keep editing</Button><Button appearance="standard" variant="danger" onClick={() => { setDirty(false); setOpen(false); setDestination(null); setGeneration(value => value + 1); if (destination) router.push(destination, { scroll: false }); }}>Discard draft</Button></div></Dialog>
  </>;
}
