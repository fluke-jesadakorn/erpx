"use client";
import {useEffect,useState,type ReactNode} from 'react';
import {registerUnsavedWork,contextIsSwitching,setContextSwitching} from '@/lib/work_projection/context-switch';
import {Dialog} from '@/components/ui/dialog';
import {Button} from '@/components/ui/foundation';

/** The dedicated public form stays inline. Only navigation confirmation is a
 * small dialog, sharing the ordinary account/context dirty-work registry. */
export function PublicMarketingDraftBoundary({dirty,children}:{dirty:boolean;children:ReactNode}){
 const [destination,setDestination]=useState<string|null>(null);
 useEffect(()=>registerUnsavedWork(()=>dirty),[dirty]);
 useEffect(()=>{
  if(!dirty)return;const source=window.location.href,state=window.history.state,navigation=(window as Window&{navigation?:EventTarget}).navigation;
  const unload=(event:BeforeUnloadEvent)=>{if(!contextIsSwitching())event.preventDefault();};
  const traverse=(event:Event)=>{const next=event as Event&{navigationType?:string;destination?:{url:string}};if(next.navigationType==='traverse'&&next.cancelable&&next.destination?.url!==source){next.preventDefault();setDestination(next.destination?.url??null);}};
  const back=(event:PopStateEvent)=>{if(window.location.href!==source){event.stopImmediatePropagation();const next=window.location.href;window.history.pushState(state,'',source);setDestination(next);}};
  const click=(event:MouseEvent)=>{if(event.defaultPrevented||event.button!==0||event.metaKey||event.ctrlKey||event.shiftKey||event.altKey||!(event.target instanceof Element))return;const anchor=event.target.closest('a');if(!anchor||anchor.target==='_blank'||anchor.hasAttribute('download')||anchor.getAttribute('href')?.startsWith('#')||anchor.href===source)return;event.preventDefault();event.stopPropagation();setDestination(anchor.href);};
  document.addEventListener('click',click,true);window.addEventListener('beforeunload',unload);window.addEventListener('popstate',back,true);navigation?.addEventListener('navigate',traverse);
  return()=>{document.removeEventListener('click',click,true);window.removeEventListener('beforeunload',unload);window.removeEventListener('popstate',back,true);navigation?.removeEventListener('navigate',traverse);};
 },[dirty]);
 return <div data-work-dirty={dirty?'true':undefined}>{children}<Dialog appearance="standard" open={destination!==null} title="Leave this request?" onClose={()=>setDestination(null)}><p>Your newer details have not been submitted. A request with an uncertain result keeps its original body and key while this form stays open.</p><div className="ui-form-actions"><Button appearance="standard" data-autofocus onClick={()=>setDestination(null)}>Keep editing</Button><Button appearance="standard" variant="danger" onClick={()=>{if(destination){setContextSwitching(true);window.location.assign(destination);}}}>Leave this form</Button></div></Dialog></div>;
}
