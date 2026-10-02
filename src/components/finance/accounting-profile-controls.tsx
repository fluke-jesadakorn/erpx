"use client";

import { useRef,useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { AccountingProfile } from "@/server/accounting-profiles";
import type { FinanceChart } from "@/components/modules/finance-chart-controls";

export function AccountingProfileControls({base,charts,profiles,canPrepare,canApprove,userId}:{base:string;charts:FinanceChart[];profiles:AccountingProfile[];canPrepare:boolean;canApprove:boolean;userId:string}){
  const router=useRouter(),[pending,setPending]=useState(false),[error,setError]=useState("");
  const attempt=useRef<{fingerprint:string;key:string}|null>(null);
  async function send(path:string,input:unknown){
    const fingerprint=path+JSON.stringify(input);if(attempt.current?.fingerprint!==fingerprint)attempt.current={fingerprint,key:crypto.randomUUID()};
    setPending(true);setError("");
    try{
      const response=await workFetch(`${base.replace("/holdings/","/api/v1/holdings/")}/finance/${path}`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.current.key},body:JSON.stringify(input)});
      const result=await response.json();if(!response.ok)throw new Error(result.error?.message??"Could not save.");
      attempt.current=null;router.refresh();
    }catch(cause){setError(cause instanceof Error?cause.message:"Could not save.");}finally{setPending(false);}
  }
  return <section className="card" aria-label="Thai accounting profile"><h2>Statutory chart and Company accounting policy</h2>
    <p className="muted">Record NPAEs eligibility and inventory/depreciation policy. A different reviewer approves the evidence. Then validate opening balances, tax and live workflows.</p>
    {error&&<p className="error" role="alert">{error}</p>}
    {profiles.map(profile=><article className="card" key={String(profile.id)}><h3>{String(profile.chart_name)} · {String(profile.status)}</h3><p>{String(profile.standard)} · Effective {String(profile.effective_date)} · Fiscal year start month {String(profile.fiscal_start_month)}</p><p>Eligibility: {String(profile.eligibility_evidence)}</p><p>Policy: {String(profile.policy_evidence)}</p>
      {profile.status==="draft"&&canApprove&&profile.prepared_by!==userId&&<form className="business-form" onSubmit={e=>{e.preventDefault();const f=new FormData(e.currentTarget);void send(`accounting-profiles/${profile.id}/${f.get("decision")}`,{expectedVersion:Number(profile.version),reason:f.get("reason")});}}><label>Review decision<select name="decision"><option value="approve">Approve</option><option value="reject">Return for preparation</option></select></label><label>Review evidence and reason<input name="reason" required minLength={10} maxLength={4000}/></label><button className="button" disabled={pending}>Record review decision</button></form>}
      {profile.review_reason&&<p>Review decision: {String(profile.review_reason)}</p>}
    </article>)}
    {canPrepare&&!profiles.some(p=>p.status==="approved")&&<form className="business-form" onSubmit={e=>{e.preventDefault();const f=new FormData(e.currentTarget);void send("accounting-profiles",{statutoryChartId:f.get("statutoryChartId"),standard:"TFRS for NPAEs",fiscalStartMonth:Number(f.get("fiscalStartMonth")),effectiveDate:f.get("effectiveDate"),eligibilityEvidence:f.get("eligibilityEvidence"),policyEvidence:f.get("policyEvidence"),taxBasis:"current_tax",inventoryPolicy:"moving_average",depreciationPolicy:"straight_line"});}}>
      <label>Statutory chart (THB)<select name="statutoryChartId" required defaultValue=""><option value="">Select chart</option>{charts.filter(c=>c.base_currency==="THB").map(chart=><option key={String(chart.id)} value={String(chart.id)}>{String(chart.name)}</option>)}</select></label>
      <label>Effective date<input name="effectiveDate" type="date" required/></label><label>Fiscal year start month<input name="fiscalStartMonth" type="number" min={1} max={12} defaultValue={1} required/></label>
      <label>Evidence of Company NPAEs eligibility<textarea name="eligibilityEvidence" required minLength={10} maxLength={4000}/></label>
      <label>Evidence of current-tax, moving-average and straight-line policies<textarea name="policyEvidence" required minLength={10} maxLength={4000}/></label>
      <button className="button" disabled={pending||!charts.some(c=>c.base_currency==="THB")}>Prepare accounting profile for review</button>
    </form>}
  </section>;
}
