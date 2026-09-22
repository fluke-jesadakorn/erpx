'use client';

import {useRef,useState} from 'react';
import {useRouter} from 'next/navigation';
import {workFetch} from '@/lib/work-fetch';

type Row=Record<string,string|number|null>;

function useCommand(base:string){
 const router=useRouter(),[busy,setBusy]=useState(false),[error,setError]=useState(''),last=useRef<{fingerprint:string;key:string}|null>(null);
 async function send(path:string,input:unknown){
  if(busy)return null;setBusy(true);setError('');const fingerprint=path+JSON.stringify(input);
  if(last.current?.fingerprint!==fingerprint)last.current={fingerprint,key:crypto.randomUUID()};
  try{const response=await workFetch(`/api/v1${base}/${path}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':last.current.key},body:JSON.stringify(input)}),body=await response.json();if(!response.ok)throw new Error(body.error?.message??'The landed cost action failed.');last.current=null;router.refresh();return body.data as Row;}
  catch(e){setError(e instanceof Error?e.message:'The landed cost action failed.');return null;}finally{setBusy(false);}
 }
 return {busy,error,send};
}

export function LandedCostCreateForm({base,fulfillment,accounts,currency}:{base:string;fulfillment:Row;accounts:Row[];currency:string}){
 const command=useCommand(base),[open,setOpen]=useState(false);
 return <section className="card">
  <div className="section-head"><div><strong>Allocate freight, duty or handling</strong><p className="muted">Capitalizes the cost across stock from this posted receipt. All received stock and lots must still be in the receiving warehouse.</p></div><button className="button secondary" type="button" onClick={()=>setOpen(v=>!v)}>{open?'Close':'Add landed cost'}</button></div>
  {open&&<form className="business-form" onSubmit={async e=>{e.preventDefault();const form=new FormData(e.currentTarget),result=await command.send('landed-costs',{fulfillmentId:String(fulfillment.id),costDate:String(form.get('costDate')),description:String(form.get('description')),allocationMethod:String(form.get('allocationMethod')),amount:String(form.get('amount')),creditAccountId:String(form.get('creditAccountId'))});if(result)setOpen(false);}}>
   <label>Cost date<input name="costDate" type="date" min={String(fulfillment.fulfillment_date??'')} defaultValue={String(fulfillment.fulfillment_date??'')} required/></label>
   <label>Description<input name="description" minLength={5} maxLength={500} placeholder="Ocean freight, import duty or handling reference" required/></label>
   <label>Allocation basis<select name="allocationMethod" defaultValue="receipt_value"><option value="receipt_value">Receipt value</option><option value="quantity">Received quantity</option></select></label>
   <label>Amount ({currency})<input name="amount" inputMode="decimal" pattern="\d{1,12}(\.\d{1,2})?" placeholder="0.00" required/></label>
   <label>Credit account<select name="creditAccountId" required defaultValue=""><option value="" disabled>Select freight accrual, duty payable or expense</option>{accounts.map(a=><option key={String(a.id)} value={String(a.id)}>{String(a.code)} · {String(a.name)} · {String(a.category)}</option>)}</select></label>
   <p className="muted">The posted journal debits inventory and credits this account after independent Company Finance approval.</p>
   {command.error&&<p className="error" role="alert">{command.error}</p>}<button className="button" disabled={command.busy}>{command.busy?'Submitting…':'Submit for review'}</button>
  </form>}
 </section>;
}

export function LandedCostActions({base,record,canApprove,canCancel}:{base:string;record:Row;canApprove:boolean;canCancel:boolean}){
 const command=useCommand(base),[reason,setReason]=useState('');const status=String(record.status),action=status==='approved'?'post':'approve';
 if(!((status==='submitted'&&(canApprove||canCancel))||(status==='approved'&&canApprove)))return null;
 const run=(next:'approve'|'reject'|'cancel'|'post')=>command.send(`landed-costs/${record.id}/${next}`,{expectedVersion:Number(record.version),reason});
 return <form className="business-form" onSubmit={e=>e.preventDefault()}><label>Decision reason<textarea value={reason} onChange={e=>setReason(e.target.value)} minLength={3} maxLength={1000} placeholder="State the evidence checked or correction required" required/></label>{command.error&&<p className="error" role="alert">{command.error}</p>}<div className="form-actions">{canApprove&&<button className="button" disabled={command.busy||reason.trim().length<3} type="button" onClick={()=>run(action)}>{action==='post'?'Post inventory value and GL':'Approve allocation'}</button>}{canApprove&&status==='submitted'&&<button className="button secondary" disabled={command.busy||reason.trim().length<3} type="button" onClick={()=>run('reject')}>Reject for correction</button>}{canCancel&&status==='submitted'&&<button className="button secondary" disabled={command.busy||reason.trim().length<3} type="button" onClick={()=>run('cancel')}>Cancel</button>}</div></form>;
}
