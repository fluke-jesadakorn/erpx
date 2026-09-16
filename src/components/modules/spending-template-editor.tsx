"use client";
import { useState } from "react";
import type { OrganizationOverview } from "@/lib/organization";
import type { SpendingException } from "@/lib/spending";
import { useStaffCommand } from "./staff-work-controls";

export function SpendingTemplateEditor({base,data}:{base:string;data:OrganizationOverview}) {
  const template=data.spendingTemplate!;
  const [exceptions,setExceptions]=useState(template.exceptions),{send,busy,error}=useStaffCommand(base);
  function update(index:number,patch:Partial<SpendingException>){setExceptions(rows=>rows.map((row,i)=>i===index?{...row,...patch}:row));}
  return <section className="card"><h2>Company spending approval template · version {template.version}</h2>
    <p>Requester → nearest higher reporting supervisor → Accounting preparation → independent Accounting approval (Level 1–4) → general ledger posting → Finance payment with bank slip.</p>
    <p className="muted">The designated CEO skips supervisor approval only. Every other exception requires a recorded reason. Changes apply to new submissions; existing routes require explicit repair.</p>
    <form onSubmit={e=>{e.preventDefault();const f=new FormData(e.currentTarget);void send("people/organization/spending-template",{exceptions,expectedVersion:template.version,reason:f.get("reason")});}}>
      {exceptions.map((row,i)=><fieldset key={i} className="business-form"><legend>Exception {i+1}</legend>
        <label>Position<select value={row.positionId??""} onChange={e=>update(i,{positionId:e.target.value||null})}><option value="">All positions for selected request type</option>{data.positions.filter(p=>p.active).map(p=><option key={p.id} value={p.id}>{p.title}</option>)}</select></label>
        <label>Request type<select value={row.requestKind??""} onChange={e=>update(i,{requestKind:(e.target.value||null) as SpendingException["requestKind"]})}><option value="">All request types for selected position</option><option value="expense">Expense claim</option><option value="purchase">Purchase request</option><option value="payment">Payment request</option></select></label>
        <label>Exception<select value={row.action} onChange={e=>update(i,{action:e.target.value as SpendingException["action"]})}><option value="named_supervisor">Use an explicit higher supervisor</option><option value="skip_supervisor">Skip organizational approval only</option></select></label>
        {row.action==="named_supervisor"&&<label>Independent supervisor<select required value={row.supervisorUserId??""} onChange={e=>update(i,{supervisorUserId:e.target.value||null})}><option value="">Select staff</option>{data.people.filter(p=>p.user_id&&p.status==="active").map(p=><option key={p.engagement_id} value={p.user_id!}>{p.legal_name} · Level {p.level_number??"unmapped"}</option>)}</select></label>}
        <label>Exception reason<input required minLength={3} maxLength={1000} value={row.reason} onChange={e=>update(i,{reason:e.target.value})}/></label>
        <button type="button" className="button secondary" onClick={()=>setExceptions(rows=>rows.filter((_,j)=>j!==i))}>Remove exception</button>
      </fieldset>)}
      <button type="button" className="button secondary" onClick={()=>setExceptions(rows=>[...rows,{positionId:null,requestKind:null,action:"named_supervisor",supervisorUserId:null,reason:""}])}>Add explicit exception</button>
      <label>Reason for this template revision<input name="reason" required minLength={3} maxLength={1000}/></label>
      <button className="button" disabled={busy}>Save company template</button>{error&&<p role="alert">{error}</p>}
    </form>
  </section>;
}
