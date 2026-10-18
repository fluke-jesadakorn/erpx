"use client";
import {useEffect,useLayoutEffect,useRef,useState,useTransition} from "react";
import {useRouter} from "@/lib/shared/destination-router";
import {announceWorkChange} from "@/lib/work_projection/work-refresh";
import {contextIsSwitching,registerUnsavedWork} from "@/lib/work_projection/context-switch";
import {createProjectStructureCommands,type ProjectStructureCurrent,type ProjectStructureAcknowledgement} from "@/lib/projects/project-structure-command";
export function useProjectStructureCommand(current:ProjectStructureCurrent){
 const router=useRouter(),[commands]=useState(()=>createProjectStructureCommands()),live=useRef<ProjectStructureCurrent|null>(null),completion=useRef<((result:ProjectStructureAcknowledgement)=>void)|undefined>(undefined);
 const [refreshing,startRefresh]=useTransition();
 const [busy,setBusy]=useState(false),[pending,setPending]=useState(false),[error,setError]=useState(""),[notice,setNotice]=useState(""),[pendingAction,setPendingAction]=useState<string|null>(null);
 useLayoutEffect(()=>{live.current=current;commands.bind(()=>live.current);return()=>{live.current=null;commands.bind(()=>null);};},[current,commands]);
 useEffect(()=>{const dirty=()=>commands.hasPending()||commands.busy(),unregister=registerUnsavedWork(dirty),unload=(event:BeforeUnloadEvent)=>{if(dirty()&&!contextIsSwitching())event.preventDefault();};window.addEventListener("beforeunload",unload);return()=>{unregister();window.removeEventListener("beforeunload",unload);};},[commands]);
 async function apply(run:()=>ReturnType<typeof commands.retry>){
  setBusy(true);setError("");setNotice("");
  try{
   const result=await run();result.assertCurrent();const unchanged=result.unchanged();
   if(!result.acknowledge())throw Error("The original Project acknowledgement was already handled.");
   result.assertCurrent();if(unchanged)completion.current?.(result.data);completion.current=undefined;
   if(unchanged)for(const form of result.completions()){result.assertCurrent();window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:form}));}
   result.assertCurrent();announceWorkChange();result.assertCurrent();startRefresh(()=>{result.assertCurrent();router.refresh();});
   if(!unchanged)setNotice("The submitted Project version was saved. Your newer draft is retained.");
   return unchanged?result.data:null;
  }catch(cause){setError(cause instanceof Error?cause.message:"Unable to save. Recover the original Project request.");return null;}
  finally{setBusy(false);setPending(commands.hasPending());setPendingAction(commands.pendingAction());}
 }
 function send(path:string,input:unknown,destination?:string,form?:HTMLFormElement|readonly HTMLFormElement[],saved?:(result:ProjectStructureAcknowledgement)=>void){
  if(commands.hasPending()||commands.busy()){setError("Recover the original Project request first. Your newer draft is retained.");return Promise.resolve(null);}
  if(path!==`projects/${encodeURIComponent(current.graph.project.id)}/structure`||destination!==undefined){setError("Unsupported Project structure operation.");return Promise.resolve(null);}
  const forms:readonly HTMLFormElement[]=form?Array.isArray(form)?form:[form as HTMLFormElement]:[];
  completion.current=saved;return apply(()=>commands.send(input,forms.map(form=>({form,formEditVersion:form.getAttribute("data-erpx-edit-version")}))));
 }
 return {send,retry:()=>apply(commands.retry),pending,pendingAction,busy:busy||refreshing,error,notice};
}
