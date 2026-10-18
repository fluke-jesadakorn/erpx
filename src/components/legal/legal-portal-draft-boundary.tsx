"use client";
import {useEffect,useState,type ReactNode} from 'react';
import {registerUnsavedWork,contextIsSwitching,setContextSwitching} from '@/lib/work_projection/context-switch';
import {Dialog} from '@/components/ui/dialog';
import {Button} from '@/components/ui/foundation';
/** Public Portal drafts share the normal account/context switch dirty guard. */
export function LegalPortalDraftBoundary({dirty,children}:{dirty:boolean;children:ReactNode}){
 const [destination,setDestination]=useState<string|null>(null);
 useEffect(()=>registerUnsavedWork(()=>dirty),[dirty]);
 useEffect(()=>{if(!dirty)return;const source=window.location.href,historyState=window.history.state,navigation=(window as Window&{navigation?:EventTarget}).navigation;
  const unload=(event:BeforeUnloadEvent)=>{if(!contextIsSwitching())event.preventDefault();};
  const traverse=(event:Event)=>{const next=event as Event&{navigationType?:string;destination?:{url:string}};if(next.navigationType==='traverse'&&next.cancelable&&next.destination?.url!==source){next.preventDefault();setDestination(next.destination?.url??null);}};
  const back=(event:PopStateEvent)=>{if(window.location.href!==source){event.stopImmediatePropagation();const target=window.location.href;window.history.pushState(historyState,'',source);setDestination(target);}};
  window.addEventListener('beforeunload',unload);window.addEventListener('popstate',back,true);navigation?.addEventListener('navigate',traverse);return()=>{window.removeEventListener('beforeunload',unload);window.removeEventListener('popstate',back,true);navigation?.removeEventListener('navigate',traverse);};
 },[dirty]);
 return <div onClickCapture={event=>{if(!dirty||event.defaultPrevented||event.button!==0||event.metaKey||event.ctrlKey||event.shiftKey||event.altKey||!(event.target instanceof Element))return;const anchor=event.target.closest('a');if(!anchor||anchor.target==='_blank'||anchor.hasAttribute('download')||anchor.getAttribute('href')?.startsWith('#'))return;const target=new URL(anchor.href,window.location.href);if(target.href===window.location.href)return;event.preventDefault();event.stopPropagation();setDestination(target.href);}}>{children}<Dialog appearance="standard" open={destination!==null} title="Leave unsaved shared-document work?" onClose={()=>setDestination(null)}><p>Your draft has not been saved. Any action with an uncertain result keeps its original key for review in this account session.</p><div className="ui-form-actions"><Button appearance="standard" data-autofocus onClick={()=>setDestination(null)}>Keep editing</Button><Button appearance="standard" variant="danger" onClick={()=>{if(destination){setContextSwitching(true);window.location.assign(destination);}}}>Leave this draft</Button></div></Dialog></div>;
}
