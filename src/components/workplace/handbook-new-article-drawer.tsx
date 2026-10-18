'use client';
import {useEffect,useRef,useState,type ReactNode} from 'react';
import {Dialog} from '@/components/ui/dialog';
import {Button} from '@/components/ui/button';
import {useRouter} from '@/lib/shared/destination-router';
import {handbookCreateReturn} from '@/lib/collaboration/handbook-navigation';

/** Creation has no record identity yet, so it owns a form drawer independently
 * of the existing-record slot's closed ID parser. */
export function HandbookNewArticleDrawer({base,returnTo,children}:{base:string;returnTo?:string;children:ReactNode}){
 const router=useRouter(),root=useRef<HTMLDivElement>(null),[confirming,setConfirming]=useState(false),[destination,setDestination]=useState<string|null>(null),closeHref=handbookCreateReturn(base,returnTo);
 const dirty=()=>!!root.current?.querySelector('[data-handbook-draft][data-work-dirty="true"]');
 useEffect(()=>{
  const source=window.location.href,historyState=window.history.state;
  function unload(event:BeforeUnloadEvent){if(dirty()){event.preventDefault();event.returnValue='';}}
  function back(event:PopStateEvent){if(dirty()&&window.location.href!==source){event.stopImmediatePropagation();const href=window.location.href;window.history.pushState(historyState,'',source);setDestination(href);setConfirming(true);}}
  const navigation=(window as Window&{navigation?:EventTarget}).navigation;
  function traverse(event:Event){const next=event as Event&{navigationType?:string;destination?:{url:string}};if(dirty()&&event.cancelable&&next.navigationType==='traverse'&&next.destination?.url!==source){event.preventDefault();setDestination(next.destination?.url??null);setConfirming(true);}}
  window.addEventListener('beforeunload',unload);window.addEventListener('popstate',back,true);navigation?.addEventListener('navigate',traverse);
  return()=>{window.removeEventListener('beforeunload',unload);window.removeEventListener('popstate',back,true);navigation?.removeEventListener('navigate',traverse);};
 },[]);
 function close(href=closeHref){if(dirty()){setDestination(href);setConfirming(true);}else router.replace(href,{scroll:false});}
 return <Dialog appearance="standard" presentation="form-drawer" open title="New handbook article" onClose={()=>close()}>
  <div ref={root} onClickCapture={event=>{
   if(!dirty()||event.button!==0||event.metaKey||event.ctrlKey||event.shiftKey||event.altKey)return;
   const link=(event.target as Element).closest<HTMLAnchorElement>('a[href]');
   if(!link||link.target==='_blank')return;
   const url=new URL(link.href,window.location.href);
   if(url.origin!==window.location.origin||url.pathname.startsWith('/api/'))return;
   event.preventDefault();close(url.pathname+url.search+url.hash);
  }}>{children}</div>
  <Dialog appearance="standard" open={confirming} title="Discard unsaved changes?" onClose={()=>setConfirming(false)}>
   <p>Your article has not been saved.</p><div className="ui-form-actions">
    <Button appearance="standard" variant="secondary" onClick={()=>setConfirming(false)}>Keep editing</Button>
    <Button appearance="standard" variant="danger" onClick={()=>{
     root.current?.querySelectorAll<HTMLFormElement>('form[data-handbook-draft]').forEach(form=>form.reset());
     setConfirming(false);router.replace(destination??closeHref,{scroll:false});
    }}>Discard article draft</Button>
   </div>
  </Dialog>
 </Dialog>;
}
