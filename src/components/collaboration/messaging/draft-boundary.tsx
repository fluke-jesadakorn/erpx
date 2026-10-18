'use client';
import {useEffect,useState,type ReactNode} from 'react';
import {registerUnsavedWork,contextIsSwitching,setContextSwitching} from '@/lib/work_projection/context-switch';
import {Dialog} from '@/components/ui/dialog';
import {Button,InlineNotice} from '@/components/ui/foundation';
export function MessagingDraftBoundary({dirty,unresolved,onDiscard,children}:{dirty:boolean;unresolved:boolean;onDiscard:()=>Promise<void>;children:ReactNode}){
 const [destination,setDestination]=useState<string|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState('');
 useEffect(()=>{const unregister=registerUnsavedWork(()=>dirty);if(!dirty)return unregister;const source=window.location.href,state=window.history.state,navigation=(window as Window&{navigation?:EventTarget}).navigation;
  const click=(event:MouseEvent)=>{if(event.defaultPrevented||event.button!==0||event.metaKey||event.ctrlKey||event.altKey||event.shiftKey||!(event.target instanceof Element))return;const link=event.target.closest<HTMLAnchorElement>('a[href]');if(!link||link.target==='_blank'||link.hasAttribute('download')||link.getAttribute('href')?.startsWith('#')||link.href===window.location.href)return;event.preventDefault();event.stopImmediatePropagation();setDestination(link.href);};
  const unload=(event:BeforeUnloadEvent)=>{if(!contextIsSwitching())event.preventDefault();};
  const back=(event:PopStateEvent)=>{if(window.location.href!==source){event.stopImmediatePropagation();const target=window.location.href;window.history.pushState(state,'',source);setDestination(target);}};
  const traverse=(event:Event)=>{const next=event as Event&{navigationType?:string;destination?:{url:string}};if(next.navigationType==='traverse'&&next.cancelable&&next.destination?.url!==source){next.preventDefault();setDestination(next.destination?.url??null);}};
  document.addEventListener('click',click,true);window.addEventListener('beforeunload',unload);window.addEventListener('popstate',back,true);navigation?.addEventListener('navigate',traverse);return()=>{unregister();document.removeEventListener('click',click,true);window.removeEventListener('beforeunload',unload);window.removeEventListener('popstate',back,true);navigation?.removeEventListener('navigate',traverse);};
 },[dirty]);
 return <>{children}<Dialog open={destination!==null} title="Leave unsaved Messages work?" onClose={()=>!busy&&setDestination(null)} dismissDisabled={busy}><p>{unresolved?'Retry the submitted action in its original account and role before leaving. Your newer draft remains here.':'Your message has not been sent. Uploaded unsent attachments will be explicitly withdrawn and retained as history.'}</p>{error&&<InlineNotice tone="danger">{error}</InlineNotice>}<div className="ui-form-actions"><Button data-autofocus disabled={busy} onClick={()=>setDestination(null)}>Keep editing</Button>{!unresolved&&<Button variant="danger" disabled={busy} onClick={async()=>{if(!destination)return;setBusy(true);try{await onDiscard();setContextSwitching(true);window.location.assign(destination);}catch(error){setError((error as Error).message);}finally{setBusy(false);}}}>Discard and leave</Button>}</div></Dialog></>;
}
