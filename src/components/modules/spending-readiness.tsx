"use client";

import { workFetch } from "@/lib/work-fetch";
import { useState } from "react";
export function SpendingReadiness({base}:{base:string}) {
  const [kind,setKind]=useState("expense"),[result,setResult]=useState(""),[busy,setBusy]=useState(false);
  async function check(){setBusy(true);try{const response=await workFetch(`/api/v1${base}/people/organization/readiness?kind=${kind}`);const body=await response.json();if(!response.ok)throw new Error(body.error?.message??"Readiness check failed.");setResult(body.data.ready?"Your staff profile and independent approval team are ready. Each request still needs its supporting details and documents.":body.data.blockedReason);}catch(e){setResult(e instanceof Error?e.message:"Readiness check failed.");}finally{setBusy(false);}}
  return <section className="card"><h2>Check my spending readiness</h2><p className="muted">Checks your reporting route and independent Accounting and Finance participants.</p><div className="business-form"><label>Request type<select value={kind} onChange={e=>{setKind(e.target.value);setResult("");}}><option value="expense">Expense claim</option><option value="purchase">Purchase request</option><option value="payment">Payment request</option></select></label><button type="button" className="button secondary" disabled={busy} onClick={()=>void check()}>{busy?"Checking…":"Check readiness"}</button></div>{result&&<p role="status">{result}</p>}</section>;
}
