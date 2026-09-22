'use client';
import {useState} from 'react';
import {useRouter} from 'next/navigation';

type Props={holdingSlug:string;bindingId:string;bindingVersion:number;actions?:Record<string,unknown>[]};
export function CustomerPortalActions({holdingSlug,bindingId,bindingVersion,actions=[]}:Props){
 const router=useRouter(),[note,setNote]=useState(''),[busy,setBusy]=useState<string|null>(null),[error,setError]=useState(''),[saved,setSaved]=useState('');
 const acknowledged=actions.some(action=>action.action==='acknowledge_delivery'),supportRequested=actions.some(action=>action.action==='request_support');
 async function submit(action:'acknowledge_delivery'|'request_support'){
  if(busy)return;setBusy(action);setError('');setSaved('');
  try{
   const response=await fetch(`/api/customer-portal/${encodeURIComponent(holdingSlug)}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':crypto.randomUUID()},body:JSON.stringify({bindingId,expectedBindingVersion:bindingVersion,action,note:action==='request_support'?note:''})});
   const body=await response.json().catch(()=>({}));
   if(!response.ok)throw new Error(body.error?.message??'The customer action could not be recorded.');
   setSaved(action==='request_support'?'Support request sent to the company.':'Delivery acknowledgement recorded.');setNote('');router.refresh();
  }catch(error){setError(error instanceof Error?error.message:'The customer action could not be recorded.');}finally{setBusy(null);}
 }
 return <div className="content-stack" aria-label="Customer actions">
   <h3>Actions</h3>
   <label>Message to the company (optional for support)<textarea value={note} onChange={event=>setNote(event.target.value)} maxLength={500} rows={3} placeholder="Describe what you need help with" /></label>
   <div className="actions"><button type="button" className="button" disabled={Boolean(busy)||acknowledged} onClick={()=>void submit('acknowledge_delivery')}>{acknowledged?'Delivery acknowledged':busy==='acknowledge_delivery'?'Recording…':'Acknowledge delivery'}</button><button type="button" className="button secondary" disabled={Boolean(busy)||supportRequested} onClick={()=>void submit('request_support')}>{supportRequested?'Support requested':busy==='request_support'?'Sending…':'Request support'}</button></div>
   {saved&&<p role="status">{saved}</p>}{error&&<p role="alert" className="error">{error}</p>}
 </div>;
}
