"use client";
import {useEffect,useLayoutEffect,useRef,useState,useTransition} from 'react';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {contextIsSwitching,registerUnsavedWork} from '@erpx/feature-work_projection/lib/context-switch';
import {createProjectEditorCommands,type ProjectEditorCurrent,type ProjectEditorReceipt} from '@erpx/feature-projects/lib/project-editor-command';
import {useProjectEditorBoundary} from '@erpx/feature-projects/components/project-editor-boundary';
type Completion={identity?:(receipt:ProjectEditorReceipt)=>void;saved?:(receipt:ProjectEditorReceipt)=>void;destination?:'projects'|'staff-tasks'|'routine-templates'};
export function useProjectEditorCommand(current:ProjectEditorCurrent){
 const boundary=useProjectEditorBoundary(),scope=boundary?.source??current.scope,router=useRouter(),[commands]=useState(()=>createProjectEditorCommands()),live=useRef<ProjectEditorCurrent|null>(null),completion=useRef<Completion>({});
 const [busy,setBusy]=useState(false),[pending,setPending]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),[refreshing,startRefresh]=useTransition();
 useLayoutEffect(()=>{live.current={...current,scope};commands.bind(()=>live.current);return()=>{live.current=null;commands.bind(()=>null);};},[current,scope,commands]);
 useLayoutEffect(()=>{boundary?.active(commands.hasPending()||commands.busy()||current.dirty);},[boundary,commands,current.dirty,busy,pending]);
 useEffect(()=>{const dirty=()=>commands.hasPending()||commands.busy()||Boolean(live.current?.dirty),unregister=registerUnsavedWork(dirty),unload=(event:BeforeUnloadEvent)=>{if(dirty()&&!contextIsSwitching())event.preventDefault();};window.addEventListener('beforeunload',unload);return()=>{unregister();window.removeEventListener('beforeunload',unload);};},[commands]);
 async function apply(run:()=>ReturnType<typeof commands.retry>){
  boundary?.active(true);setBusy(true);setError('');setNotice('');
  try{
   const result=await run();result.assertCurrent();const unchanged=result.unchanged();if(!result.acknowledge())throw Error('The original Project acknowledgement was already handled.');
   result.assertCurrent();completion.current.identity?.(result.data);result.assertCurrent();if(unchanged)completion.current.saved?.(result.data);
   const target=completion.current.destination;completion.current={};const form=result.completion();if(unchanged&&form){result.assertCurrent();window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:form}));}
   result.assertCurrent();announceWorkChange();result.assertCurrent();startRefresh(()=>{result.assertCurrent();if(unchanged&&target)router.push(canonicalUiHref(`${current.base}/${target}/${encodeURIComponent(result.data.id)}`));else router.refresh();});
   if(!unchanged)setNotice('The submitted Project record was saved. Its native identity is retained with your newer draft.');return unchanged?result.data:null;
  }catch(cause){setError(cause instanceof Error?cause.message:'Unable to save. Recover the original Project request.');return null;}
  finally{setBusy(false);setPending(commands.hasPending());}
 }
 function send(path:string,input:unknown,form:HTMLFormElement,callbacks:Completion={}){
  if(commands.hasPending()||commands.busy()||busy||refreshing){setError('Recover the original Project request first. Your newer draft is retained.');return Promise.resolve(null);}
  completion.current=callbacks;return apply(()=>commands.send(path,input,{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}));
 }
 return {send,retry:()=>apply(commands.retry),pending,busy:busy||refreshing,error,notice,scope};
}
