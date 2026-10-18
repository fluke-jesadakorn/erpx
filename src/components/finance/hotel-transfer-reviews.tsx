"use client";
import { useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { Button, Field, Select } from "@/components/ui/foundation";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { pmsFinanceOverview } from "@/server/modules/pms/public/pms/finance";
export function HotelTransferReviews({base,reviews}:{base:string;reviews:Awaited<ReturnType<typeof pmsFinanceOverview>>["reviews"]}) {
  const [error,setError]=useState(""),[busy,setBusy]=useState(false),router=useRouter();
  return <section><h3>Verify hotel bank transfers</h3>{error&&<p role="alert">{error}</p>}
    {reviews.filter(row=>row.state==="pending").map(row=><FormDisclosure key={`${row.id}:${row.version}:${row.property_version}`}><summary>{row.reference} · {row.amount_minor/100} {row.currency}</summary>
      <p>{row.reasonDisabled??"Match this transfer to an unused bank transaction."}</p>
      <form onSubmit={event=>{
        event.preventDefault();const form=new FormData(event.currentTarget),bank=row.bankEvidence.find(item=>item.id===form.get("bank"));if(!bank)return;
        setBusy(true);setError("");
        void workFetch(`/api/v1${base}/pms`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":crypto.randomUUID()},body:JSON.stringify({operation:"payment.verify_transfer",propertyId:row.property_id,expectedVersion:row.property_version,input:{id:row.id,expectedTransferVersion:row.version,bankTransactionId:bank.id,bankVersion:bank.version}})}).then(async response=>{const body=await response.json();if(!response.ok)throw new Error(body.error?.message??"Verification failed");router.refresh();}).catch(error=>setError(error.message)).finally(()=>setBusy(false));
      }}>
        <Field>Bank evidence<Select name="bank" required><option value="">Select bank transaction</option>{row.bankEvidence.map(bank=><option key={bank.id} value={bank.id}>{bank.transaction_date} · {bank.external_id} · {bank.amount_minor/100}</option>)}</Select></Field>
        <Button type="submit" disabled={busy||!row.canVerify||!row.bankEvidence.length}>Verify and post receipt</Button>
      </form>
    </FormDisclosure>)}
  </section>;
}
