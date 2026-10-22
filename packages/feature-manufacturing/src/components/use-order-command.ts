"use client";
import {useEffect,useLayoutEffect,useState,useTransition} from 'react';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {contextIsSwitching,registerUnsavedWork} from '@erpx/feature-work_projection/lib/context-switch';
import {createManufacturingOrderCommands,manufacturingOrderImage,manufacturingOrderOwner,type ManufacturingOrderCurrent} from '@erpx/feature-manufacturing/lib/order-command';

/** This mounted native order owns one immutable attempt independently of its
 * editable form and changing server snapshot. It does not promise reload storage. */
export function useManufacturingOrderCommand(source:ManufacturingOrderCurrent|null){
 const router=useRouter(),[runtime]=useState(()=>{
  let current:ManufacturingOrderCurrent|null=null,listener:(()=>void)|null=null,savedDraft:{owner:string;version:number;refreshRequested:boolean}|null=null;
  const commands=createManufacturingOrderCommands();commands.bind(()=>current);
  function releaseRefreshed(){if(savedDraft?.refreshRequested&&current&&manufacturingOrderOwner(current)===savedDraft.owner&&Number(current.order.version)>=savedDraft.version){savedDraft=null;return true;}return false;}
  return{commands,commit(value:ManufacturingOrderCurrent|null){current=value;return releaseRefreshed();},holdDraft(owner:string,version:number){savedDraft={owner,version,refreshRequested:false};},needsRefresh(owner:string){return savedDraft?.owner===owner;},requestRefresh(owner:string){if(savedDraft?.owner===owner)savedDraft.refreshRequested=true;releaseRefreshed();},dispose(){current=null;},subscribe(next:()=>void){listener=next;return()=>{if(listener===next)listener=null;};},sync(){listener?.();}};
 }),[busy,setBusy]=useState(false),[uncertain,setUncertain]=useState(false),[needsRefresh,setNeedsRefresh]=useState(false),[error,setError]=useState<{owner:string;text:string}|null>(null),[notice,setNotice]=useState<{owner:string;text:string}|null>(null),[refreshing,startRefresh]=useTransition();
 const {commands}=runtime,stamp=source?manufacturingOrderImage(source):'',owner=source?manufacturingOrderOwner(source):'';
 useLayoutEffect(()=>{if(runtime.commit(source))runtime.sync();});
 useLayoutEffect(()=>()=>runtime.dispose(),[runtime]);
 useEffect(()=>{
  let disposed=false,sequence=0;
  const sync=()=>{const token=++sequence;let assertOwner:()=>void;try{assertOwner=commands.captureOwner();}catch{return;}queueMicrotask(()=>{if(disposed||token!==sequence)return;try{assertOwner();}catch{return;}setBusy(commands.busy());setUncertain(commands.hasPending());setNeedsRefresh(runtime.needsRefresh(owner));});};
  const unsubscribe=runtime.subscribe(sync),held=()=>commands.busy()||commands.hasPending(),unregister=registerUnsavedWork(held),unload=(event:BeforeUnloadEvent)=>{if(held()&&!contextIsSwitching())event.preventDefault();};
  sync();window.addEventListener('beforeunload',unload);for(const name of ['focus','erpx:work-context-changed','erpx:context-switch-started'])window.addEventListener(name,sync);
  return()=>{disposed=true;++sequence;unsubscribe();unregister();window.removeEventListener('beforeunload',unload);for(const name of ['focus','erpx:work-context-changed','erpx:context-switch-started'])window.removeEventListener(name,sync);};
 },[commands,runtime,stamp,owner]);
 async function execute(input?:unknown,form?:HTMLFormElement,retry=false){
  let assertCurrent:()=>void,assertOwner:()=>void;try{assertCurrent=commands.captureCurrent();assertOwner=commands.captureOwner();}catch{return;}
  if(commands.busy())return;if(!retry&&runtime.needsRefresh(owner)){setError({owner,text:'The original change is saved. Retain or explicitly discard your newer draft before reviewing the current order.'});return;}setBusy(true);setError(null);setNotice(null);
  try{
   if(!retry&&!form)throw Error('Submit from the original Manufacturing form so its fields and completion remain bound.');
   const submitted=retry?commands.retry():commands.send(input,{form:form!,formEditVersion:form!.getAttribute('data-erpx-edit-version')});runtime.sync();
   const outcome=await submitted;outcome.assertCurrent();assertCurrent();const unchanged=outcome.unchanged();if(!outcome.acknowledge())throw Error('The original Manufacturing acknowledgement was already handled.');outcome.assertCurrent();assertCurrent();
   const completion=outcome.completion();
   if(unchanged&&completion){window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,manufacturingAcknowledged:true}}));outcome.assertCurrent();assertCurrent();window.dispatchEvent(new CustomEvent('erpx:manufacturing-command-accepted',{detail:completion}));}
   else{runtime.holdDraft(owner,outcome.data.version);setNeedsRefresh(true);setNotice({owner,text:'The original Manufacturing change was saved. Your newer fields and evidence remain unsubmitted. Explicitly discard this draft before refreshing the saved order.'});return;}
   outcome.assertCurrent();assertCurrent();announceWorkChange();outcome.assertCurrent();assertCurrent();startRefresh(()=>{outcome.assertCurrent();assertCurrent();router.refresh();});
  }catch(cause){try{assertCurrent();setError({owner,text:cause instanceof Error?cause.message:'Recover the original Manufacturing change before submitting again.'});}catch{/* Original attempt remains held; a stale owner cannot publish an error. */}}
  finally{try{assertOwner();setBusy(commands.busy());setUncertain(commands.hasPending());}catch{/* No state settlement across a changed native Work or stable owner. */}runtime.sync();}
 }
 function refreshSaved(){if(!runtime.needsRefresh(owner))return;let assertCurrent:()=>void;try{assertCurrent=commands.captureCurrent();assertCurrent();}catch{return;}runtime.requestRefresh(owner);runtime.sync();setNotice(null);setError(null);startRefresh(()=>{assertCurrent();router.refresh();});}
 return{enabled:!!source,send:(input:unknown,form:HTMLFormElement)=>execute(input,form),retry:()=>execute(undefined,undefined,true),refreshSaved,error:error?.owner===owner?error.text:'',notice:notice?.owner===owner?notice.text:'',needsRefresh,pending:busy||refreshing,uncertain,retained:()=>commands.hasPending()||commands.busy()||runtime.needsRefresh(owner),pendingForCurrent:()=>commands.pendingForCurrent()};
}
