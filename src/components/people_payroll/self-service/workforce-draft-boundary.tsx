"use client";
import {useEffect,useRef,useState,type ReactNode} from 'react';
import {registerUnsavedWork,contextIsSwitching,setContextSwitching} from '@/lib/work_projection/context-switch';
import {Dialog} from '@/components/ui/dialog';
import {Button} from '@/components/ui/foundation';
/** Only these Workforce forms/drafts participate; payroll and list filters keep
 * their own owners. Matching original edit revisions clear completed forms. */
export function WorkforceDraftBoundary({dirty=false,children}:{dirty?:boolean;children:ReactNode}){
 const root=useRef<HTMLDivElement>(null),forms=useRef(new Set<HTMLFormElement>()),[destination,setDestination]=useState<string|null>(null);
 useEffect(()=>{const current=forms.current,isDirty=()=>dirty||[...current].some(form=>form.isConnected),source=window.location.href,historyState=window.history.state,navigation=(window as Window&{navigation?:EventTarget}).navigation,unregister=registerUnsavedWork(isDirty);
  const saved=(event:Event)=>{const data=(event as CustomEvent<{workforceAcknowledged?:boolean;form?:HTMLFormElement;formEditVersion?:string|null}>).detail;if(data?.workforceAcknowledged&&data.form&&data.formEditVersion===data.form.getAttribute('data-erpx-edit-version'))current.delete(data.form);};
  const click=(event:MouseEvent)=>{if(!isDirty()||event.defaultPrevented||event.button!==0||event.metaKey||event.ctrlKey||event.shiftKey||event.altKey||!(event.target instanceof Element))return;const anchor=event.target.closest<HTMLAnchorElement>('a[href]');if(!anchor||anchor.target==='_blank'||anchor.hasAttribute('download')||anchor.getAttribute('href')?.startsWith('#'))return;const target=new URL(anchor.href);if(!['http:','https:'].includes(target.protocol)||target.href===window.location.href)return;event.preventDefault();event.stopImmediatePropagation();setDestination(target.href);};
  const unload=(event:BeforeUnloadEvent)=>{if(isDirty()&&!contextIsSwitching())event.preventDefault();};
  const traverse=(event:Event)=>{const next=event as Event&{navigationType?:string;destination?:{url:string}};if(isDirty()&&next.navigationType==='traverse'&&next.cancelable&&next.destination?.url!==source){next.preventDefault();setDestination(next.destination?.url??null);}};
  const back=(event:PopStateEvent)=>{if(isDirty()&&window.location.href!==source){event.stopImmediatePropagation();const target=window.location.href;window.history.pushState(historyState,'',source);setDestination(target);}};
  window.addEventListener('erpx:record-command-saved',saved);document.addEventListener('click',click,true);window.addEventListener('beforeunload',unload);window.addEventListener('popstate',back,true);navigation?.addEventListener('navigate',traverse);return()=>{unregister();window.removeEventListener('erpx:record-command-saved',saved);document.removeEventListener('click',click,true);window.removeEventListener('beforeunload',unload);window.removeEventListener('popstate',back,true);navigation?.removeEventListener('navigate',traverse);};
 },[dirty]);
 const edited=(target:EventTarget)=>{if(!(target instanceof Element))return;const form=target.closest('form');if(form&&root.current?.contains(form)){form.setAttribute('data-erpx-edit-version',crypto.randomUUID());forms.current.add(form);}};
 return <div ref={root} onInputCapture={event=>edited(event.target)} onChangeCapture={event=>edited(event.target)}>{children}<Dialog appearance="standard" open={destination!==null} title="Leave unsaved Workforce work?" onClose={()=>setDestination(null)}><p>Your current draft is retained here. An uncertain action keeps its original request for review in the original account and role.</p><div className="ui-form-actions"><Button appearance="standard" data-autofocus onClick={()=>setDestination(null)}>Keep editing</Button><Button appearance="standard" variant="danger" onClick={()=>{if(destination){setContextSwitching(true);window.location.assign(destination);}}}>Leave this draft</Button></div></Dialog></div>;
}
