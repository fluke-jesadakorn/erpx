"use client";
import {useState} from "react";
import {useRouter} from "next/navigation";
import {financeCommandFetch as workFetch} from "@/lib/finance-command-fetch";
import {Button,Card,Field,FormLayout,TextArea,InlineNotice,ActionLink} from "@/components/ui/foundation";
type Row=Record<string,string|number|null>;
export function FinanceDueControls({base,rows,canWrite}:{base:string;rows:Row[];canWrite:boolean}){
 const router=useRouter(),[busy,setBusy]=useState(false),[message,setMessage]=useState("");return <>
 {message&&<InlineNotice>{message}</InlineNotice>}
 {!rows.length&&<p className="muted">Choose a preparation date in billing, revenue recognition, asset depreciation or financing payment forms. Due work prepares a draft for review.</p>}
 {rows.map(r=>{const result=r.result_json?JSON.parse(String(r.result_json)) as Record<string,unknown>:null,source=r.kind==='billing'||r.kind==='recognition'?'/finance/contracts':r.kind==='asset_depreciation'?'/finance/asset-policies':'/finance/instruments',href=result?.invoiceId?`/finance/invoices/${encodeURIComponent(String(result.invoiceId))}`:source;return <Card appearance="standard" key={String(r.id)}>
 <h2>{String(r.kind).replaceAll('_',' ')} · {String(r.run_on)}</h2><p>{String(r.status)} · {String(r.reason)}</p>
 {r.last_message&&<InlineNotice tone="danger">{String(r.last_message)}</InlineNotice>}
 <ActionLink appearance="standard" href={`${base}${href}`}>{result?'Open prepared draft':'Open source and resolve missing information'}</ActionLink>
 {canWrite&&['scheduled','blocked'].includes(String(r.status))&&<FormLayout onSubmit={async e=>{e.preventDefault();const f=new FormData(e.currentTarget,(e.nativeEvent as SubmitEvent).submitter);setBusy(true);try{const response=await workFetch(`${base.replace('/holdings/','/api/v1/holdings/')}/finance/due-drafts/${r.id}/${f.get('action')}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':crypto.randomUUID()},body:JSON.stringify({expectedVersion:r.version,reason:f.get('reason')})}),body=await response.json();if(!response.ok)throw new Error(body.error?.message??'The schedule could not be changed.');setMessage('Schedule updated.');router.refresh();}catch(error){setMessage(error instanceof Error?error.message:'The schedule could not be changed.');}finally{setBusy(false);}}}>
 <Field appearance="standard">Change reason<TextArea appearance="standard" name="reason" required minLength={3}/></Field>
 {r.status==='blocked'&&<Button appearance="standard" clientManaged type="submit" name="action" value="retry" disabled={busy}>Retry after fixing source</Button>}
 <Button appearance="standard" clientManaged type="submit" name="action" value="cancel" disabled={busy}>Cancel scheduled draft</Button>
 </FormLayout>}
 </Card>;})}</>;
}
