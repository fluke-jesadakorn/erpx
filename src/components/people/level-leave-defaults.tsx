"use client";
import { useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";

type Level={id:string;name:string;level_number:number};
type Default={level_id:string;version:number;annual_days:number;sick_days:number};
export function LevelLeaveDefaults({base,levels,defaults}:{base:string;levels:Level[];defaults:Default[]}){
  const router=useRouter(),[busy,setBusy]=useState<string|null>(null),[error,setError]=useState("");
  return <section className="card"><h2>Leave defaults by level</h2><p className="muted">Annual and sick days are tracked. Saving creates a new policy version; existing assignments keep their policy history. A level change during the year takes effect for leave on the next 1 January.</p><div className="content-stack">{levels.map(level=>{
    const current=defaults.find(item=>item.level_id===level.id);
    return <form key={`${level.id}-${current?.version??0}`} className="business-form" onSubmit={async event=>{event.preventDefault();const fields=new FormData(event.currentTarget);setBusy(level.id);setError("");try{
      const response=await workFetch(`/api/v1${base}/people/organization/levels/${level.id}/default-leave`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":crypto.randomUUID()},body:JSON.stringify({annualDays:Number(fields.get("annual")),sickDays:Number(fields.get("sick")),expectedVersion:current?.version??0})});
      const result=await response.json();if(!response.ok)throw Error(result.error?.message??"Unable to save.");router.refresh();
    }catch(cause){setError(cause instanceof Error?cause.message:"Unable to save.");}finally{setBusy(null);}}}>
      <strong>Level {level.level_number} · {level.name} {current?`· v${current.version}`:"· no default"}</strong>
      <label>Annual days<input name="annual" type="number" min="0" max="365" step="0.5" required defaultValue={current?.annual_days??0}/></label>
      <label>Sick days<input name="sick" type="number" min="0" max="365" step="0.5" required defaultValue={current?.sick_days??0}/></label>
      <button className="button secondary" disabled={busy!==null}>Save new version</button>
    </form>;
  })}</div>{error&&<p role="alert" className="error">{error}</p>}</section>;
}
