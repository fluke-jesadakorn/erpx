"use client";

import { workFetch } from "@/lib/work-fetch";
import { useState } from "react";
import { useRouter } from "next/navigation";
export function InvoiceSlipUpload({base,invoiceId,onUploaded}:{base:string;invoiceId:string;onUploaded:(id:string)=>void}) {
  const router=useRouter(),[busy,setBusy]=useState(false),[error,setError]=useState(""),[name,setName]=useState("");
  async function upload(file:File){
    setBusy(true);setError("");onUploaded("");
    try {
      const reserved=await workFetch(`/api/v1${base}/uploads`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":crypto.randomUUID()},body:JSON.stringify({ownerType:"invoice_payment",ownerId:invoiceId,purpose:"bank_payment_slip",filename:file.name,mediaType:file.type,bytes:file.size})});
      const intent=await reserved.json();if(!reserved.ok)throw new Error(intent.error?.message??"Could not reserve upload.");
      const response=await workFetch(intent.data.uploadUrl,{method:"PUT",headers:{"Content-Type":file.type},body:file});
      const result=await response.json();if(!response.ok)throw new Error(result.error?.message??"Could not upload the slip.");
      onUploaded(result.data.id);setName(file.name);router.refresh();
    }catch(e){setError(e instanceof Error?e.message:"Upload failed. Try again.");}finally{setBusy(false);}
  }
  return <label>Bank payment slip<input type="file" accept="image/jpeg,image/png,image/webp,application/pdf" disabled={busy} onChange={e=>{const file=e.target.files?.[0];if(file)void upload(file);}}/>{busy?"Uploading…":name?`Attached: ${name}`:"A separate bank slip is required for each partial payment."}{error&&<span role="alert">{error}</span>}</label>;
}
