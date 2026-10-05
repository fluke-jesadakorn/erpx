"use client";
import { Suspense, useEffect, useRef, useState, type ReactNode } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { EmbeddedRecord } from "./record-context";
import { recordDrawerHref, withoutRecordDrawer, type RecordTarget } from "@/lib/record-drawer";
import { registerUnsavedWork } from "@/lib/context-switch";
import "./record-drawer.css";

let originTrigger:HTMLElement|null=null;
type HistoryNavigationEvent=Event & {navigationType?:string;destination?:{url:string}};
export function RecordDrawer({ title, fullHref, children }: { title: string; fullHref: string; children: ReactNode }) {
  const router = useRouter(), query = useSearchParams();
  const [contentNode, setContentNode] = useState<HTMLDivElement | null>(null), [stale, setStale] = useState(false);
  const [dirty, setDirty] = useState(false), [confirming, setConfirming] = useState(false), [recordTitle, setRecordTitle] = useState(title);
  const destination = useRef<string | null>(null), latestUrl = useRef(""), trigger = useRef<HTMLElement | null>(null);
  const latestHistoryState=useRef<unknown>(null);
  const dirtyRef=useRef(false);
  const editedForms=useRef(new Set<Element>());
  const open = !!query.get("detailKind") && !!query.get("detailId");
  function focusTarget() {
    if(trigger.current?.isConnected)return trigger.current;
    const href=trigger.current?.getAttribute('href');
    return href?Array.from(document.querySelectorAll<HTMLElement>('a[data-record-link="true"]')).find(link=>link.getAttribute('href')===href)??null:null;
  }
  function navigate(url: string) {
    if (dirty) { destination.current = url; setConfirming(true); }
    else router.push(url, { scroll: false });
  }
  function close() { navigate(withoutRecordDrawer(window.location.href)); }
  useEffect(() => {
    latestUrl.current = window.location.href;
    const candidate=document.querySelector<HTMLElement>('[data-record-trigger="true"]');
    candidate?.removeAttribute('data-record-trigger');
    if(candidate&&!candidate.closest('[data-record-drawer-content]'))originTrigger=candidate;
    trigger.current=originTrigger?.isConnected?originTrigger:candidate;
    return () => { requestAnimationFrame(() => {
      if(!new URLSearchParams(window.location.search).has('detailKind'))focusTarget()?.focus({preventScroll:true});
    }); };
  }, []);
  useEffect(() => { latestUrl.current = window.location.href; latestHistoryState.current=window.history.state; }, [query]);
  useEffect(() => registerUnsavedWork(() => dirty), [dirty]);
  useEffect(() => {
    function updateDirty(){
      for(const form of editedForms.current)if(!form.isConnected)editedForms.current.delete(form);
      dirtyRef.current=editedForms.current.size>0;setDirty(dirtyRef.current);
    }
    function edited(event: Event) {
      if(event.target instanceof Element&&!event.target.matches('input[type="search"]')){
        const form=event.target.closest('form, [contenteditable="true"]');
        if(form){editedForms.current.add(form);updateDirty();}
      }
    }
    function saved(event:Event) {
      const detail=(event as CustomEvent<{form?:Element|null;root?:Element|null}>).detail;
      for(const form of editedForms.current)if(form===detail?.form||detail?.root?.contains(form))editedForms.current.delete(form);
      updateDirty();
    }
    function requested(event: Event) {
      const detail = (event as CustomEvent<{ target?: RecordTarget; href?: string }>).detail;
      destination.current = detail.href ?? (detail.target ? recordDrawerHref(window.location.href, detail.target) : null); setConfirming(true);
    }
    function unload(event: BeforeUnloadEvent) { if (dirtyRef.current) { event.preventDefault(); event.returnValue = ""; } }
    function back(event: PopStateEvent) {
      if (dirtyRef.current && window.location.href !== latestUrl.current) {
        event.stopImmediatePropagation();
        destination.current = window.location.href;
        window.history.pushState(latestHistoryState.current, "", latestUrl.current);
        setConfirming(true);
      }
    }
    // Cancel supported browser traversals before changing the route; popstate
    // remains the fallback for browsers without the Navigation API.
    const navigation=(window as Window & {navigation?:EventTarget}).navigation;
    function traverse(event:Event){
      const historyEvent=event as HistoryNavigationEvent;
      if(dirtyRef.current&&event.cancelable&&historyEvent.navigationType==='traverse'&&historyEvent.destination?.url!==latestUrl.current){
        event.preventDefault();destination.current=historyEvent.destination?.url??null;setConfirming(true);
      }
    }
    const node = contentNode;
    const formObserver=new MutationObserver(updateDirty);
    if(node)formObserver.observe(node,{childList:true,subtree:true});
    navigation?.addEventListener('navigate',traverse);
    node?.addEventListener("input", edited); node?.addEventListener("change", edited);
    window.addEventListener("erpx:record-command-saved", saved);
    window.addEventListener("erpx:record-saved", saved);
    window.addEventListener("erpx:record-navigate", requested);
    window.addEventListener("beforeunload", unload); window.addEventListener("popstate", back,true);
    return () => { formObserver.disconnect();navigation?.removeEventListener('navigate',traverse);node?.removeEventListener("input", edited); node?.removeEventListener("change", edited); window.removeEventListener("erpx:record-command-saved", saved); window.removeEventListener("erpx:record-saved", saved); window.removeEventListener("erpx:record-navigate", requested); window.removeEventListener("beforeunload", unload); window.removeEventListener("popstate", back,true); };
  }, [contentNode]);
  useEffect(() => {
    const node = contentNode;
    const observer = new MutationObserver(() => {
      const heading = (node?.querySelector("h1")??node?.querySelector("[data-record-heading],strong,h2"))?.textContent?.trim();
      if (heading) setRecordTitle(heading);
    });
    if (node) observer.observe(node, { subtree: true, childList: true });
    const heading = (node?.querySelector("h1")??node?.querySelector("[data-record-heading],strong,h2"))?.textContent?.trim();
    if (heading) queueMicrotask(() => setRecordTitle(heading));
    return () => observer.disconnect();
  }, [contentNode]);
  useEffect(() => {
    const boundary = document.querySelector("[data-work-revision]");
    const observer = new MutationObserver(() => setStale(boundary?.getAttribute("data-work-stale") === "true"));
    if (boundary) observer.observe(boundary, { attributes: true, attributeFilter: ["data-work-stale"] });
    return () => observer.disconnect();
  }, []);
  return <Dialog appearance="standard" open={open} title={recordTitle} onClose={close} finalFocus={focusTarget} presentation="form-drawer" data-testid="record-detail-drawer">
    <div className="record-drawer-toolbar"><a href={fullHref} onClick={event => { if (dirty && !event.metaKey && !event.ctrlKey) { event.preventDefault(); destination.current = fullHref; setConfirming(true); } }}>Open full page ↗</a><span role="status">{dirty ? "Unsaved changes" : ""}</span></div>
    {stale && <p role="alert">Your work role changed. Reload this page to review access to the record.</p>}
    <div ref={setContentNode} hidden={stale} inert={stale} data-record-drawer-content data-record-dirty={dirty}>
      <EmbeddedRecord value={true}><Suspense fallback={<p role="status">Loading record details…</p>}>{children}</Suspense></EmbeddedRecord>
    </div>
    <Dialog appearance="standard" open={confirming} title="Discard unsaved changes?" onClose={() => setConfirming(false)}>
      <p>Your changes have not been saved.</p><div className="ui-form-actions"><Button appearance="standard" variant="secondary" onClick={() => setConfirming(false)}>Keep editing</Button><Button appearance="standard" variant="danger" onClick={() => { dirtyRef.current=false;setDirty(false); setConfirming(false); if (destination.current) router.push(destination.current, { scroll: false }); }}>Discard changes</Button></div>
    </Dialog>
  </Dialog>;
}
