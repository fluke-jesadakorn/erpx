'use client';
import {contextIsSwitching,registerUnsavedWork} from './context-switch';
/** Selecting a detail or hash keeps the same Calendar owner. Business filters,
 * dates and all other query/path changes remain departure boundaries. */
export function sameCalendarCommandView(currentHref:string,nextHref:string){
 const current=new URL(currentHref),next=new URL(nextHref,current);
 if(current.origin!==next.origin||current.pathname!==next.pathname)return false;
 current.searchParams.delete('event');next.searchParams.delete('event');current.searchParams.sort();next.searchParams.sort();
 return current.searchParams.toString()===next.searchParams.toString();
}
/** Uses public navigation events only. Without a cancelable Navigation API,
 * beforeunload remains available but same-document Back cannot be cancelled
 * safely; this owner never rewrites Next history state or invents entries. */
export function installCalendarPendingNavigation(held:()=>boolean,requestDeparture:(href:string)=>void){
 const unregister=registerUnsavedWork(held);
 const block=(href:string)=>{if(!held()||sameCalendarCommandView(window.location.href,href))return false;requestDeparture(href);return true;};
 const unload=(event:BeforeUnloadEvent)=>{if(held()&&!contextIsSwitching()){event.preventDefault();event.returnValue='';}};
 const click=(event:MouseEvent)=>{
  if(event.defaultPrevented||event.button!==0||event.metaKey||event.ctrlKey||event.altKey||event.shiftKey)return;
  const link=event.target instanceof Element?event.target.closest<HTMLAnchorElement>('a[href]'):null;
  if(!link||link.hasAttribute('data-product-switch')||link.hasAttribute('download')||link.target&&link.target!=='_self')return;
  const next=new URL(link.href,window.location.href);
  if(next.origin!==window.location.origin||!['http:','https:'].includes(next.protocol))return;
  if(block(next.href)){event.preventDefault();event.stopImmediatePropagation();}
 };
 const traverse=(event:Event)=>{
  const next=event as Event&{navigationType?:string;destination?:{url?:string;sameDocument?:boolean}};
  if(next.navigationType==='traverse'&&!event.defaultPrevented&&event.cancelable&&next.destination?.sameDocument===true&&next.destination.url&&block(next.destination.url))event.preventDefault();
 };
 const navigation=(window as Window&{navigation?:EventTarget}).navigation;
 window.addEventListener('beforeunload',unload);document.addEventListener('click',click,true);navigation?.addEventListener('navigate',traverse);
 return()=>{unregister();window.removeEventListener('beforeunload',unload);document.removeEventListener('click',click,true);navigation?.removeEventListener('navigate',traverse);};
}
