'use client';
import {useState} from 'react';
import {useRouter} from 'next/navigation';
import {workFetch} from '@/lib/work-fetch';
import {minorToDecimal} from '@/lib/finance-money';

type Refund={id:string;amount_minor:number;status:string;refund_state:string;last_error:string|null;created_at:number};
export function BookingRefunds({base,rows,canApprove}:{base:string;rows:Refund[];canApprove:boolean}){
 const router=useRouter(),[error,setError]=useState(''),[busy,setBusy]=useState('');
 if(!rows.length)return <p>No online booking refunds require review.</p>;
 return <section className="card"><h2>Online booking refunds</h2>{error&&<p role="alert">{error}</p>}<div className="business-table-wrap"><table className="business-table"><thead><tr><th>Booking payment</th><th>Amount</th><th>Status</th><th>Provider review</th></tr></thead><tbody>{rows.map(row=><tr key={row.id}><td>{row.id.slice(0,8)}</td><td>THB {minorToDecimal(Number(row.amount_minor))}</td><td>{row.refund_state}{row.last_error&&<p>{row.last_error}</p>}</td><td>{canApprove&&row.refund_state==='requested'?<button className="button" disabled={Boolean(busy)} onClick={async()=>{setBusy(row.id);setError('');try{const response=await workFetch(`/api/v1${base}/pms/booking-refunds`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({id:row.id})});const body=await response.json();if(!response.ok)throw new Error(body.error?.message??'Approval failed');router.refresh();}catch(error){setError(error instanceof Error?error.message:'Approval failed');}finally{setBusy('');}}}>Approve Stripe refund</button>:row.refund_state==='processing'?'Waiting for Stripe':row.refund_state==='succeeded'?'Confirmed by Stripe':'Finance review'}</td></tr>)}</tbody></table></div></section>;
}
