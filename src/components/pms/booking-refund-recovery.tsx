'use client';
import {FrontLocalizedTree} from '@/components/front/front-language';
import {useState} from 'react';
import {Button,Field,TextInput} from '@/components/ui/foundation';
import {FormDisclosure} from '@/components/ui/form-disclosure';
import {workFetch} from "@/lib/work_projection/work-fetch";
export function BookingRefundRecovery({base,id,onSaved,receipt=false}:{base:string;id:string;onSaved:()=>void;receipt?:boolean}){
 const [busy,setBusy]=useState(false),[error,setError]=useState('');
 return <FrontLocalizedTree><FormDisclosure appearance="standard" dismissDisabled={busy} error={error}><summary>{receipt?'Recover original receipt':'Review and retry failed refund'}</summary><form onSubmit={async e=>{e.preventDefault();const reason=String(new FormData(e.currentTarget).get('reason')??'');setBusy(true);setError('');try{const response=await workFetch(`/api/v1${base}/pms/booking-refunds`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({id,action:receipt?'recover-receipt':'retry',reason})});const result=await response.json();if(!response.ok)throw new Error(result.error?.message??'Refund recovery unavailable');onSaved();}catch(e){setError(e instanceof Error?e.message:'Refund recovery unavailable');}finally{setBusy(false);}}}><p>{receipt?'Verify and post the original Stripe receipt before approving its refund.':'Stripe must confirm the previous refund failed before another attempt can start. The refund liability remains outstanding.'}</p><Field appearance="standard">Recovery reason<TextInput appearance="standard" name="reason" required minLength={3}/></Field><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>{receipt?'Recover original receipt':'Approve retry'}</Button></form></FormDisclosure></FrontLocalizedTree>;
}
