"use client";
import { createPortal } from "react-dom";
import { WorkspaceDetail } from "@/components/ui/workspace";
import { useWorkspaceOverlay } from "@/components/ui/workspace-overlay";
import { useRecordWorkspace } from "./record-workspace";
import { Suspense, useEffect, useRef, useState, type ReactNode } from "react";
import { useSearchParams } from "@/lib/shared/destination-search";
import { useRouter } from "@/lib/shared/destination-router";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { EmbeddedRecord } from "./record-context";
import { recordDrawerHref, withoutRecordDrawer, type RecordTarget } from "@/lib/records/record-drawer";
import { registerUnsavedWork } from "@/lib/work_projection/context-switch";
import "./record-drawer.css";

let originTrigger:HTMLElement|null=null;
type HistoryNavigationEvent=Event & {navigationType?:string;destination?:{url:string}};
type RetainedDraftField={key:string;label:string;value:string;checked?:boolean};
type TrackedDraft={editVersion:string|null;orphaned:boolean;fields:RetainedDraftField[]};
function snapshotForm(form:Element,changedControl?:Element):RetainedDraftField[]{
  if(typeof form.querySelectorAll!=='function')return [];
  return Array.from(form.querySelectorAll<HTMLInputElement|HTMLSelectElement|HTMLTextAreaElement>('input,select,textarea'))
    .flatMap((control,index)=>{
      if(control.disabled)return [];
      const isInput=control.tagName.toLowerCase()==='input',inputType=isInput?(control as HTMLInputElement).type.toLowerCase():'';
      if(['password','file','hidden','submit','reset','button','image'].includes(inputType))return [];
      const isCheck=isInput&&(inputType==='checkbox'||inputType==='radio');
      let changed=control===changedControl;
      if(!changed&&isCheck)changed=(control as HTMLInputElement).checked!==(control as HTMLInputElement).defaultChecked;
      else if(!changed&&control.tagName.toLowerCase()==='select')changed=Array.from((control as HTMLSelectElement).options).some(option=>option.selected!==option.defaultSelected);
      else if(!changed)changed=control.value!==(control as HTMLInputElement|HTMLTextAreaElement).defaultValue;
      if(!changed)return [];
      const name=control.name||control.id||`field-${index+1}`;
      const label=Array.from(control.labels??[]).map(item=>item.textContent?.trim()).find(Boolean)
        ??control.getAttribute('aria-label')?.trim()??name;
      if(isInput&&(inputType==='checkbox'||inputType==='radio')){
        const checked=(control as HTMLInputElement).checked;
        return [{key:`${name}:${index}`,label,value:checked?'Selected':'Not selected',checked}];
      }
      return [{key:name,label,value:control.value}];
    });
}
export function RecordDrawer({ title, fullHref, closeHref, children }: { title: string; fullHref: string; closeHref?:string; children: ReactNode }) {
  const router = useRouter(), query = useSearchParams();
  const {host}=useRecordWorkspace(), {editors}=useWorkspaceOverlay();
  const [contentNode, setContentNode] = useState<HTMLDivElement | null>(null), [stale, setStale] = useState(false);
  const [dirty, setDirty] = useState(false), [confirming, setConfirming] = useState(false), [recordTitle, setRecordTitle] = useState(title),[retainedDrafts,setRetainedDrafts]=useState<TrackedDraft[]>([]);
  const destination = useRef<string | null>(null),destinationPreservesQuery=useRef(false), latestUrl = useRef(""), trigger = useRef<HTMLElement | null>(null);
  const latestHistoryState=useRef<unknown>(null);
  const dirtyRef=useRef(false);
  const editedForms=useRef(new Map<Element,TrackedDraft>()),retainedSignature=useRef('');
  const open = !!query.get("detailKind") && !!query.get("detailId");
  function focusTarget() {
    if(trigger.current?.isConnected)return trigger.current;
    const href=trigger.current?.getAttribute('href');
    return href?Array.from(document.querySelectorAll<HTMLElement>('a[data-record-link="true"]')).find(link=>link.getAttribute('href')===href)??null:null;
  }
  function navigate(url: string,preserveQuery=false) {
    if (dirty) { destination.current = url;destinationPreservesQuery.current=preserveQuery; setConfirming(true); }
    else if(preserveQuery)router.pushPreservingQuery(url,{scroll:false});
    else router.push(url, { scroll: false });
  }
  function close() { navigate(closeHref??withoutRecordDrawer(window.location.href),true); }
  useEffect(() => {
    latestUrl.current = window.location.href;
    const candidate=document.querySelector<HTMLElement>('[data-record-trigger="true"]');
    candidate?.removeAttribute('data-record-trigger');
    if(candidate&&!candidate.closest('[data-record-drawer-content]'))originTrigger=candidate;
    trigger.current=originTrigger?.isConnected?originTrigger:candidate;
    return () => { requestAnimationFrame(() => {
      if(!document.querySelector('[data-record-drawer-content]'))focusTarget()?.focus({preventScroll:true});
    }); };
  }, []);
  useEffect(() => { latestUrl.current = window.location.href; latestHistoryState.current=window.history.state; }, [query]);
  useEffect(() => registerUnsavedWork(() => dirty), [dirty]);
  useEffect(() => {
    function updateDirty(){
      const retained:TrackedDraft[]=[];
      for(const [form,draft] of editedForms.current){
        if(!form.isConnected)draft.orphaned=true;
        if(draft.orphaned)retained.push(draft);
      }
      const nextDirty=editedForms.current.size>0;
      dirtyRef.current=nextDirty;
      setDirty(current=>current===nextDirty?current:nextDirty);
      const signature=JSON.stringify(retained);
      if(signature!==retainedSignature.current){retainedSignature.current=signature;setRetainedDrafts(retained);}
    }
    function edited(event: Event) {
      if(event.target instanceof Element&&!event.target.matches('input[type="search"]')){
        const form=event.target.closest('form, [contenteditable="true"]');
        if(form){
          const editVersion=form.matches('form')?crypto.randomUUID():null;
          if(editVersion)form.setAttribute('data-erpx-edit-version',editVersion);
          editedForms.current.set(form,{editVersion,orphaned:false,fields:form.matches('form')?snapshotForm(form,event.target):[]});updateDirty();
        }
      }
    }
    function finalized(event:Event){
      if(event.target instanceof Element&&!event.target.matches('input[type="search"]')){
        const form=event.target.closest('form, [contenteditable="true"]');
        // Owner boundaries may stamp a form at document level. Window receives
        // the same bubbling event after those listeners, so capture the final
        // submitted version without trusting later attribute mutations.
        const draft=form?editedForms.current.get(form):undefined;
        if(draft&&form?.matches('form'))draft.editVersion=form.getAttribute('data-erpx-edit-version');
      }
    }
    function saved(event:Event) {
      const detail=(event as CustomEvent<{form?:Element|null;formEditVersion?:string|null;root?:Element|null}>).detail;
      for(const [form,draft] of editedForms.current){
        // A form-scoped completion cannot mark newer local fields clean. Tracked
        // forms receive a drawer-owned stamp even without an owner adapter; only
        // an explicit root event represents a scoped discard. Compare with the
        // locally retained stamp so owner-side attribute changes cannot make an
        // older acknowledgement look current.
        const acknowledged=form===detail?.form&&typeof draft.editVersion==='string'&&draft.editVersion!==''&&draft.editVersion===detail.formEditVersion;
        if((event.type==='erpx:record-command-saved'||event.type==='erpx:record-saved')&&acknowledged){
          editedForms.current.delete(form);
        }
        else if(event.type==='erpx:record-saved'&&detail?.form===undefined&&detail?.root?.contains(form))editedForms.current.delete(form);
      }
      updateDirty();
    }
    function requested(event: Event) {
      const detail = (event as CustomEvent<{ target?: RecordTarget; href?: string }>).detail;
      destination.current = detail.href ?? (detail.target ? recordDrawerHref(window.location.href, detail.target) : null);destinationPreservesQuery.current=false; setConfirming(true);
    }
    function unload(event: BeforeUnloadEvent) { if (dirtyRef.current) { event.preventDefault(); event.returnValue = ""; } }
    function back(event: PopStateEvent) {
      if (dirtyRef.current && window.location.href !== latestUrl.current) {
        event.stopImmediatePropagation();
        destination.current = window.location.href;destinationPreservesQuery.current=false;
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
        event.preventDefault();destination.current=historyEvent.destination?.url??null;destinationPreservesQuery.current=false;setConfirming(true);
      }
    }
    const node = contentNode;
    const formObserver=new MutationObserver(updateDirty);
    if(node)formObserver.observe(node,{childList:true,subtree:true});
    navigation?.addEventListener('navigate',traverse);
    node?.addEventListener("input", edited); node?.addEventListener("change", edited);
    window.addEventListener("input",finalized);window.addEventListener("change",finalized);
    window.addEventListener("erpx:record-command-saved", saved);
    window.addEventListener("erpx:record-saved", saved);
    window.addEventListener("erpx:record-navigate", requested);
    window.addEventListener("beforeunload", unload); window.addEventListener("popstate", back,true);
    return () => { formObserver.disconnect();navigation?.removeEventListener('navigate',traverse);node?.removeEventListener("input", edited); node?.removeEventListener("change", edited);window.removeEventListener("input",finalized);window.removeEventListener("change",finalized); window.removeEventListener("erpx:record-command-saved", saved); window.removeEventListener("erpx:record-saved", saved); window.removeEventListener("erpx:record-navigate", requested); window.removeEventListener("beforeunload", unload); window.removeEventListener("popstate", back,true); };
  }, [contentNode]);
  useEffect(() => {
    const node = contentNode;
    function updateTitle(){
      const heading=node?.querySelector<HTMLElement>('h1')??node?.querySelector<HTMLElement>('[data-record-heading],strong,h2');
      const text=heading?.textContent?.trim();if(text)setRecordTitle(text);
      if(host&&heading?.tagName==='H1')heading.setAttribute('data-workspace-record-title','');
    }
    const observer=new MutationObserver(updateTitle);
    if(node)observer.observe(node,{subtree:true,childList:true});
    queueMicrotask(updateTitle);
    return ()=>{observer.disconnect();node?.querySelectorAll('[data-workspace-record-title]').forEach(heading=>heading.removeAttribute('data-workspace-record-title'));};
  }, [contentNode,host]);
  useEffect(() => {
    const boundary = document.querySelector("[data-work-revision]");
    const observer = new MutationObserver(() => setStale(boundary?.getAttribute("data-work-stale") === "true"));
    if (boundary) observer.observe(boundary, { attributes: true, attributeFilter: ["data-work-stale"] });
    return () => observer.disconnect();
  }, []);
  const body=<>
    <div className="record-drawer-toolbar"><a href={fullHref} target="_blank" rel="noopener">Open in new tab ↗</a><span role="status">{dirty ? "Unsaved changes" : ""}</span></div>
    {stale && <p role="alert">Your work role changed. Reload this page to review access to the record.</p>}
    <div ref={setContentNode} hidden={stale} inert={stale} data-record-drawer-content data-record-dirty={dirty}>
      <EmbeddedRecord value={true}><Suspense fallback={<p role="status">Loading record details…</p>}>{children}</Suspense></EmbeddedRecord>
    </div>
    {retainedDrafts.length>0&&<section aria-live="polite" data-record-drawer-retained-draft>
      <h3>Your newer unsaved input is retained</h3>
      <p>The record action changed while you were editing. This newer input was not submitted.</p>
      {retainedDrafts.map((draft,index)=><dl key={index}>{draft.fields.map(field=><div key={field.key}><dt>{field.label}</dt><dd><output data-record-drawer-draft-field={field.key}>{field.value||'—'}</output></dd></div>)}</dl>)}
    </section>}
    <Dialog appearance="standard" open={confirming} title="Discard unsaved changes?" onClose={() => setConfirming(false)}>
      <p>Your changes have not been saved.</p><div className="ui-form-actions"><Button appearance="standard" variant="secondary" onClick={() => setConfirming(false)}>Keep editing</Button><Button appearance="standard" variant="danger" onClick={() => { dirtyRef.current=false;editedForms.current.clear();retainedSignature.current='';setRetainedDrafts([]);setDirty(false); setConfirming(false); if (destination.current)destinationPreservesQuery.current?router.pushPreservingQuery(destination.current,{scroll:false}):router.push(destination.current, { scroll: false }); }}>Discard changes</Button></div>
    </Dialog>
  </>;
  if(host)return createPortal(<div data-record-workspace-detail data-editor-open={editors>0||undefined}><WorkspaceDetail suspended={editors>0} open={open} title={recordTitle} onClose={close} data-testid="record-detail-drawer">{body}</WorkspaceDetail></div>,host.node);
  return <Dialog workspaceRole="detail" appearance="standard" open={open} title={recordTitle} onClose={close} finalFocus={focusTarget} closeLabel="Close details" presentation="form-drawer" data-testid="record-detail-drawer">{body}</Dialog>;
}
