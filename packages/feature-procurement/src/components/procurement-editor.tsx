"use client";
import {useEffect,useRef,useState,type ReactNode} from "react";
import {DraftFormDrawer} from "@erpx/ui/components/draft-form-drawer";
import {Button,InlineNotice} from "@erpx/ui/components/foundation";
import {Dialog} from "@erpx/ui/components/dialog";
import {registerUnsavedWork,contextIsSwitching,setContextSwitching} from "@erpx/feature-work_projection/lib/context-switch";
/** Keep the original form and edit revision through refresh, failed commands,
 * later acknowledgements and package changes. Only explicit discard resets it. */
export function ProcurementEditor({title,children,error,busy,disabled=false,onDiscard,onBegin}:{title:string;children:ReactNode;error?:string;busy?:boolean;disabled?:boolean;onDiscard:()=>void;onBegin?:()=>void}){
 const [open,setOpen]=useState(false),[dirty,setDirty]=useState(false),[destination,setDestination]=useState<string|null>(null),root=useRef<HTMLDivElement>(null),trigger=useRef<HTMLButtonElement>(null);
 useEffect(()=>registerUnsavedWork(()=>dirty),[dirty]);
 useEffect(()=>{
  if(!dirty)return;const source=window.location.href,historyState=window.history.state,navigation=(window as Window&{navigation?:EventTarget}).navigation;
  const unload=(event:BeforeUnloadEvent)=>{if(!contextIsSwitching())event.preventDefault();};
  const traverse=(event:Event)=>{const next=event as Event&{navigationType?:string;destination?:{url:string}};if(next.navigationType==="traverse"&&next.cancelable&&next.destination?.url!==source){next.preventDefault();setDestination(next.destination?.url??null);}};
  const back=(event:PopStateEvent)=>{if(window.location.href!==source){event.stopImmediatePropagation();const target=window.location.href;window.history.pushState(historyState,"",source);setDestination(target);}};
  window.addEventListener("beforeunload",unload);window.addEventListener("popstate",back,true);navigation?.addEventListener("navigate",traverse);
  return()=>{window.removeEventListener("beforeunload",unload);window.removeEventListener("popstate",back,true);navigation?.removeEventListener("navigate",traverse);};
 },[dirty]);
 useEffect(()=>{const saved=(event:Event)=>{const detail=(event as CustomEvent<{form?:HTMLFormElement;formEditVersion?:string|null}>).detail;if(detail?.form&&root.current?.contains(detail.form)&&detail.formEditVersion===detail.form.getAttribute("data-erpx-edit-version")){setDirty(false);setOpen(false);onDiscard();}};window.addEventListener("erpx:record-command-saved",saved);return()=>window.removeEventListener("erpx:record-command-saved",saved);},[onDiscard]);
 const edited=(target:EventTarget)=>{if(!(target instanceof Element))return;const form=target.closest("form");if(form)form.setAttribute("data-erpx-edit-version",crypto.randomUUID());setDirty(true);};
 return <><Button ref={trigger} appearance="standard" type="button" disabled={disabled&&!dirty} onClick={()=>{if(!open&&!dirty)onBegin?.();setOpen(true);}}>{title}</Button><DraftFormDrawer appearance="standard" title={title} open={open} dirty={dirty} retainFormContents={dirty} dismissDisabled={busy} finalFocus={()=>trigger.current} onClose={()=>setOpen(false)} onDiscard={()=>{onDiscard();setDirty(false);setOpen(false);}}><div ref={root} onInputCapture={event=>edited(event.target)} onChangeCapture={event=>edited(event.target)} onClickCapture={event=>{if(event.target instanceof Element&&event.target.closest("[data-procurement-edit]"))edited(event.target);}}>{error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{disabled&&<InlineNotice>Your draft is retained. This action is currently unavailable.</InlineNotice>}{children}</div></DraftFormDrawer><Dialog appearance="standard" open={destination!==null} title="Leave unsaved purchasing work?" onClose={()=>setDestination(null)}><p>Your purchasing draft has not been saved.</p><div className="ui-form-actions"><Button appearance="standard" data-autofocus onClick={()=>setDestination(null)}>Keep editing</Button><Button appearance="standard" variant="danger" onClick={()=>{if(!destination)return;setContextSwitching(true);window.location.assign(destination);}}>Discard and leave</Button></div></Dialog></>;
}
