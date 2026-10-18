"use client";
import {useEffect,useRef,useState,type ComponentPropsWithRef} from "react";
import {QualityDisclosure,QualityForm} from "./quality-draft-boundary";
import {qualityFormUnchanged} from "@/lib/quality/quality-command-fetch";
import type {WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
/** Current-page long editor. The submitted revision and source version stay
 * captured; a response cannot dismiss fields edited while it was in flight. */
export function QualityTool({title,version=0,pending,onDiscard=()=>undefined,children,...props}:ComponentPropsWithRef<"form">&{title:string;version?:number;pending:boolean;onDiscard?:()=>void}){
 const [open,setOpen]=useState(false),[sourceVersion,setSourceVersion]=useState(version),[generation,setGeneration]=useState(0),root=useRef<HTMLDivElement>(null);
 useEffect(()=>{const saved=(event:Event)=>{const completion=(event as CustomEvent<WorkCommandCompletion>).detail;if(completion?.form&&root.current?.contains(completion.form)&&qualityFormUnchanged(completion)){setOpen(false);onDiscard();setGeneration(value=>value+1);}};window.addEventListener("erpx:quality-command-complete",saved);return()=>window.removeEventListener("erpx:quality-command-complete",saved);},[onDiscard]);
 const changed=version>sourceVersion;
 return <QualityDisclosure drawerTitle={title} open={open} onOpenChange={setOpen} onBeforeOpen={()=>setSourceVersion(version)} dismissDisabled={pending} onDiscard={()=>{onDiscard();setGeneration(value=>value+1);}}><summary>{title}</summary><div ref={root}>
  {changed&&<p role="status">This source changed while the editor was open. Your draft is retained; review the current source before submitting another change.</p>}
  <QualityForm {...props} key={generation} onSubmit={event=>{if(changed){event.preventDefault();return;}props.onSubmit?.(event);}}><input key="source-version" type="hidden" name="sourceVersion" value={sourceVersion}/><fieldset key="fields" disabled={changed} style={{display:"contents"}}>{children}</fieldset></QualityForm>
 </div></QualityDisclosure>;
}
