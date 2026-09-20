"use client";

import { workFetch } from "@/lib/work-fetch";
import { useRef,useState } from "react";
import { useRouter } from "next/navigation";

type Row=Record<string,string|number|null>;

export function PaymentCorrection({base,payment,periods}:{base:string;payment:Row;periods:Row[]}){
  const router=useRouter(),attempt=useRef<{body:string;key:string}|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState("");
  async function submit(form:HTMLFormElement){
    const values=new FormData(form),body=JSON.stringify({expectedVersion:Number(payment.version),periodId:values.get("periodId"),entryDate:values.get("entryDate"),reason:values.get("reason")});
    if(attempt.current?.body!==body)attempt.current={body,key:crypto.randomUUID()};setBusy(true);setError("");
    try{const response=await workFetch(`${base.replace("/holdings/","/api/v1/holdings/")}/finance/payments/${payment.id}/reverse`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.current.key},body}),result=await response.json() as {error?:{message:string}};if(!response.ok)throw new Error(result.error?.message??"Unable to reverse the payment.");attempt.current=null;router.refresh();}
    catch(cause){setError(cause instanceof Error?cause.message:"Unable to reverse the payment.");}
    finally{setBusy(false);}
  }
  return <section className="card"><h2>Correct posted payment</h2><p className="muted">Creates a posted reversing journal, restores every invoice allocation, and preserves the original payment as voided evidence. Reconciled bank items must be unmatched first.</p>{error&&<p className="error" role="alert">{error}</p>}<form className="business-form" onSubmit={event=>{event.preventDefault();void submit(event.currentTarget);}}><label>Correction period<select name="periodId" required defaultValue=""><option value="">Select open period</option>{periods.map(period=><option value={String(period.id)} key={String(period.id)}>{String(period.name)}</option>)}</select></label><label>Correction date<input name="entryDate" type="date" required defaultValue={new Date().toISOString().slice(0,10)}/></label><label>Correction evidence<input name="reason" required minLength={3} maxLength={500}/></label><button className="button secondary" disabled={busy}>{busy?"Reversing…":"Reverse payment and allocations"}</button></form></section>;
}
