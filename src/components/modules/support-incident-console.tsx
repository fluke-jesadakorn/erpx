'use client';
import {useRef,useState} from 'react';
import {useRouter} from 'next/navigation';
import {workFetch} from '@/lib/work-fetch';
import type {incidentDetail} from '@/server/support-incidents';
type Person={id:string;name:string};
export function SupportIncidentConsole({base,source,recipients=[],detail}:{base:string;source?:{id:string;version:number};recipients?:Person[];detail?:Awaited<ReturnType<typeof incidentDetail>>}) {
 const router=useRouter(),[busy,setBusy]=useState(false),[error,setError]=useState(''),last=useRef<{key:string;fingerprint:string}|null>(null);
 async function send(input:unknown) {if(busy)return;setBusy(true);setError('');const fingerprint=JSON.stringify(input);if(last.current?.fingerprint!==fingerprint)last.current={key:crypto.randomUUID(),fingerprint};try{const response=await workFetch(`/api/v1${base}/support/incidents`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':last.current.key},body:JSON.stringify(input)}),result=await response.json();if(!response.ok){if(response.status<500)last.current=null;throw new Error(result.error?.message??'Unable to record incident action.');}last.current=null;router.push(`${base}/support/incidents/${result.data.id}`);router.refresh();}catch(e){setError(e instanceof Error?e.message:'Retry this action.');}finally{setBusy(false);}}
 const reason=<label>Evidence and reason<textarea name="reason" required minLength={3} maxLength={4000}/></label>;
 const owner=(people:Person[],value='')=><label>Incident owner<select name="ownerId" defaultValue={value} required><option value="">Choose eligible owner</option>{people.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select></label>;
 const r=detail?.row,status=String(r?.status??'');
 function form(action:string,title:string,extra?:React.ReactNode){return <form className="card business-form" onSubmit={e=>{e.preventDefault();const f=new FormData(e.currentTarget),data=Object.fromEntries(f);void send({...data,id:r!.id,action,expectedVersion:Number(r!.version),...(action==='link'?{ticketVersion:Number(detail!.candidates.find(t=>t.id===f.get('ticketId'))?.version)}:{})});}}><h2>{title}</h2>{extra}{reason}<button className="button" disabled={busy}>{title}</button></form>;}
 return <>{error&&<p role="alert">{error}</p>}{source&&<form className="card business-form" onSubmit={e=>{e.preventDefault();const f=new FormData(e.currentTarget);void send({...Object.fromEntries(f),action:'create',ticketId:source.id,ticketVersion:source.version});}}><h2>Declare shared incident</h2><label>Incident title<input name="title" minLength={3} maxLength={160} required/></label><label>Customer impact<textarea name="impact" minLength={3} maxLength={4000} required/></label><label>Severity<select name="severity"><option value="normal">Normal</option><option value="high">High</option><option value="critical">Critical</option></select></label>{owner(recipients)}<label>Next update due<input name="dueDate" type="date" required/></label><button className="button" disabled={busy}>Declare shared incident</button></form>}
 {detail?.canWork&&status==='open'&&form('start','Start investigation')}
 {detail?.canWork&&['open','investigating'].includes(status)&&form('update','Record investigation update')}
 {detail?.canWork&&status==='investigating'&&form('mitigate','Propose mitigation',<><label>Root cause<textarea name="rootCause" required minLength={3} maxLength={4000}/></label><label>Mitigation evidence<textarea name="mitigation" required minLength={3} maxLength={4000}/></label></>)}
 {detail?.canReview&&status==='mitigated'&&form('close','Confirm incident mitigation')}
 {detail?.canManage&&status==='mitigated'&&form('reject','Request further investigation')}
 {detail?.canManage&&status==='closed'&&form('reopen','Reopen incident')}
 {detail?.canManage&&['open','investigating','mitigated'].includes(status)&&<>{form('reassign','Reassign incident',<>{owner(detail.recipients,String(r!.owner_id))}<label>Next update due<input name="dueDate" type="date" defaultValue={String(r!.due_date)} required/></label></>)}{form('cancel','Cancel incident with evidence')}</>}
 {detail?.canManage&&['open','investigating'].includes(status)&&detail.links.length>1&&form('unlink','Correct an incorrectly linked case',<label>Case to unlink<select name="ticketId" required><option value="">Choose linked case</option>{detail.links.map(t=><option key={String(t.id)} value={String(t.id)}>{String(t.title)}</option>)}</select></label>)}
 {detail?.canManage&&['open','investigating'].includes(status)&&detail.candidates.length>0&&form('link','Link affected customer case',<label>Customer case in the same scope<select name="ticketId" required><option value="">Choose customer case</option>{detail.candidates.map(t=><option key={String(t.id)} value={String(t.id)}>{String(t.title)}</option>)}</select></label>)}
 </>;
}
