"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";

type Detail={kind:"expense"|"leave"|"overtime"|"request";id:string;version:number;step:number;originalApprover:string};
export function DepartmentOverrideDecision({base,detail,returnTo=""}:{base:string;detail:Detail;returnTo?:string}){
  const router=useRouter(),[reason,setReason]=useState(""),[busy,setBusy]=useState(false),[error,setError]=useState("");
  const attempt=useRef<{body:string;key:string}|null>(null);
  async function decide(outcome:"approve"|"reject"){
    const body=JSON.stringify({kind:detail.kind,id:detail.id,expectedVersion:detail.version,outcome,reason:reason.trim()});
    if(attempt.current?.body!==body)attempt.current={body,key:crypto.randomUUID()};
    setBusy(true);setError("");
    try{
      const response=await workFetch(`/api/v1${base}/authority/decisions`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.current.key},body});
      const result=await response.json();
      if(!response.ok)throw new Error(result.error?.message??"Decision could not be recorded.");
      attempt.current=null;window.dispatchEvent(new Event("erpx:work-changed"));router.push(`${base}/work${returnTo?`?${returnTo}`:""}`);router.refresh();
    }catch(cause){setError(cause instanceof Error?cause.message:"Decision could not be recorded.");}
    finally{setBusy(false);}
  }
  return <section className="card"><h2>Record decision</h2>
    <label>Override reason <textarea value={reason} onChange={event=>setReason(event.target.value)} required minLength={3} maxLength={2048}/></label>
    <div className="form-actions"><button className="button" disabled={busy||reason.trim().length<3} onClick={()=>void decide("approve")}>Approve current step</button>
      <button className="button secondary" disabled={busy||reason.trim().length<3} onClick={()=>void decide("reject")}>Reject current step</button></div>
    {error&&<p role="alert">{error}</p>}
  </section>;
}
