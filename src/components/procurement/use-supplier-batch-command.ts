"use client";
import {useEffect,useLayoutEffect,useState,useTransition} from 'react';
import {useRouter} from '@/lib/shared/destination-router';
import {canonicalUiHref} from '@/lib/work_projection/destination-registry';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {contextIsSwitching,registerUnsavedWork} from '@/lib/work_projection/context-switch';
import {acknowledgeFinanceForm,useFinanceDraftScope} from '@/components/finance/finance-draft-boundary';
import {createSupplierBatchOwnerCommands,type SupplierBatchAttempt,type SupplierBatchReceipt,type SupplierBatchCurrent} from '@/lib/procurement/supplier-batch-owner-command';

export function useSupplierBatchCommand(source:SupplierBatchCurrent,onLockChange?:(locked:boolean)=>void){
 const router=useRouter(),draftScope=useFinanceDraftScope();
 const [runtime]=useState(()=>{
  let committed:SupplierBatchCurrent|null=null,listener:(()=>void)|null=null;
  const commands=createSupplierBatchOwnerCommands(()=>committed);
  return{commands,commit(value:SupplierBatchCurrent){committed=value;},dispose(){committed=null;},
   lease(full=true){let cancelled=false;const guard=full?commands.captureCurrent():commands.captureOwner();return{current(){if(cancelled)return false;try{guard();return true;}catch{return false;}},cancel(){cancelled=true;}};},
   subscribe(next:()=>void){listener=next;return()=>{if(listener===next)listener=null;};},sync(){listener?.();},
  };
 });
 const {commands}=runtime,[busy,setBusy]=useState(false),[refreshing,startRefresh]=useTransition(),[error,setError]=useState(''),[notice,setNotice]=useState(''),[retryAttempt,setRetryAttempt]=useState<SupplierBatchAttempt|null>(null),[retainedElsewhere,setRetainedElsewhere]=useState(false);
 const stamp=JSON.stringify(source),working=busy||refreshing;
 useLayoutEffect(()=>{runtime.commit(source);});
 useLayoutEffect(()=>()=>runtime.dispose(),[runtime]);
 useEffect(()=>{
  let cancelled=false,sequence=0;
  const synchronize=()=>{const token=++sequence;let lease:ReturnType<typeof runtime.lease>;try{lease=runtime.lease(false);}catch{return;}queueMicrotask(()=>{if(cancelled||token!==sequence||!lease.current())return;const pending=commands.pending();setRetryAttempt(pending);setRetainedElsewhere(commands.hasRetained()&&!pending);setBusy(commands.busy());onLockChange?.(commands.hasRetained()||commands.busy());lease.cancel();});};
  const unsubscribe=runtime.subscribe(synchronize),unregister=registerUnsavedWork(()=>commands.hasRetained()||commands.busy()),unload=(event:BeforeUnloadEvent)=>{if((commands.hasRetained()||commands.busy())&&!contextIsSwitching())event.preventDefault();};synchronize();window.addEventListener('beforeunload',unload);for(const event of ['focus','erpx:work-context-changed','erpx:context-switch-started'])window.addEventListener(event,synchronize);
  return()=>{cancelled=true;++sequence;unsubscribe();unregister();window.removeEventListener('beforeunload',unload);for(const event of ['focus','erpx:work-context-changed','erpx:context-switch-started'])window.removeEventListener(event,synchronize);};
 },[commands,runtime,stamp,onLockChange]);
 async function execute(path?:string,input?:unknown,form?:HTMLFormElement,onSaved?:(value:SupplierBatchReceipt,unchanged:boolean)=>void){
  if(working||commands.busy())return null;let lease:ReturnType<typeof runtime.lease>;try{lease=runtime.lease();}catch{setError('The original supplier source is currently unavailable. Its request and draft remain retained.');return null;}if(!lease.current())return null;
  setBusy(true);setError('');setNotice('');onLockChange?.(true);
  try{
   const saved=path===undefined?await commands.retry():await commands.run(path,input,form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined,onSaved);saved.assertCurrent();if(!lease.current())return null;
   const acknowledged=saved.acknowledge();if(!acknowledged)throw Error('The original supplier acknowledgement was already consumed.');saved.assertCurrent();if(!lease.current())return null;
   if(!acknowledged.unchanged){setNotice('The original supplier action is confirmed. Your newer unsubmitted fields remain here.');return saved.data;}
   acknowledgeFinanceForm(acknowledged.completion);
   saved.assertCurrent();if(!lease.current())return null;announceWorkChange();saved.assertCurrent();if(!lease.current())return null;
   if(acknowledged.created&&acknowledged.unchanged)router.push(canonicalUiHref(`${acknowledged.base}/finance/supplier-batches/${encodeURIComponent(saved.data.id)}`));
   else startRefresh(()=>{saved.assertCurrent();if(lease.current())router.refresh();});
   return saved.data;
  }catch(cause){if(lease.current())setError(cause instanceof Error?cause.message:'The original supplier outcome is unconfirmed. Retry it unchanged.');return null;}
  finally{lease.cancel();runtime.sync();}
 }
 async function recoverExport(onSaved:(value:{content:string;sha256:string})=>void){
  if(working||commands.busy())return;let lease:ReturnType<typeof runtime.lease>;try{lease=runtime.lease();}catch{return;}setBusy(true);setError('');
  try{const saved=await commands.readExport();saved.assertCurrent();if(lease.current())onSaved(saved.data);}
  catch(cause){if(lease.current())setError(cause instanceof Error?cause.message:'Original export is unavailable.');}
  finally{if(lease.current())setBusy(false);lease.cancel();runtime.sync();}
 }
 function discardAndRefresh(action:()=>void){if(commands.busy()||commands.hasRetained())return false;let guard:()=>void;try{guard=commands.captureOwner();guard();}catch{return false;}action();guard();commands.reviewDraft();startRefresh(()=>{guard();router.refresh();});return true;}
 return{discardAndRefresh,captureCurrent:commands.captureCurrent,send:(path:string,input:unknown,form?:HTMLFormElement,onSaved?:(value:SupplierBatchReceipt,unchanged:boolean)=>void)=>execute(path,input,form,onSaved),retry:()=>execute(),recoverExport,retryAttempt,uncertain:!!retryAttempt,retainedElsewhere,busy:working,error,notice,setError,draftScope};
}
