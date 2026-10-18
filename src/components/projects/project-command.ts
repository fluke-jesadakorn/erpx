"use client";
import {useRef,useState,useTransition} from "react";
import {useRouter} from "@/lib/shared/destination-router";
import {canonicalUiHref} from "@/lib/work_projection/destination-registry";
import {announceWorkChange} from "@/lib/work_projection/work-refresh";
import {createProjectCommandFetch,projectFormUnchanged,type ProjectCommandAcknowledgement} from "@/lib/projects/project-command";
export function useProjectCommand(base:string){
 const router=useRouter(),request=useRef<ReturnType<typeof createProjectCommandFetch>|null>(null),pending=useRef(false),[busy,setBusy]=useState(false),[refreshing,startRefresh]=useTransition(),[error,setError]=useState("");
 async function send(path:string,input:unknown,destination?:string,form?:HTMLFormElement|readonly HTMLFormElement[],acknowledged?:(result:ProjectCommandAcknowledgement)=>void){
  if(pending.current)return null;pending.current=true;setBusy(true);setError("");
  const forms=form?(Array.isArray(form)?form:[form as HTMLFormElement]):[document.activeElement?.closest("form")].filter((value):value is HTMLFormElement=>!!value),completions=[...new Set(forms)].map(form=>({form,formEditVersion:form.getAttribute("data-erpx-edit-version")})),completion=completions[0];
  try{
   request.current??=createProjectCommandFetch();const result=await request.current(`/api/v1${base}/${path}`,input,completion);acknowledged?.(result);
   if(completions.some(completion=>!projectFormUnchanged(completion))){setError("The submitted Project version was saved. Your newer edits remain here for review.");startRefresh(()=>router.refresh());return null;}
   for(const completion of completions)window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:completion}));announceWorkChange();
   startRefresh(()=>{if(destination)router.push(canonicalUiHref(`${base}/${destination}/${result.id}`));else router.refresh();});return result;
  }catch(error){setError(error instanceof Error?error.message:"Unable to save. Retry the same Project action.");return null;}
  finally{pending.current=false;setBusy(false);}
 }
 return {send,busy:busy||refreshing,error};
}
