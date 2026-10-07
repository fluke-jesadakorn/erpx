"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import {useRef,useState} from 'react';
import {useRouter} from 'next/navigation';
import {Button,Field,Select,TextArea,TextInput} from '@/components/ui/foundation';
import {PreferredDateTimeField} from '@/components/time/preferred-date-time-field';
import {workFetch} from '@/lib/work-fetch';
import type {maintenanceWorkspace} from "@/server/modules/maintenance/public/asset-maintenance";
import type {preventivePlanDetail} from "@/server/modules/maintenance/public/maintenance-preventive";
import { FormLayout } from "@/components/ui/foundation";

export function MaintenancePreventiveConsole({base,data,detail}:{base:string;data?:Awaited<ReturnType<typeof maintenanceWorkspace>>;detail?:Awaited<ReturnType<typeof preventivePlanDetail>>}){
 const router=useRouter(),last=useRef<{body:string;key:string}|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),p=detail?.plan;
 async function send(input:unknown){if(busy)return;setBusy(true);setError('');const body=JSON.stringify(input);if(last.current?.body!==body)last.current={body,key:crypto.randomUUID()};try{
  const response=await workFetch(`/api/v1${base}/maintenance/plans`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':last.current.key},body}),result=await response.json();
  if(!response.ok){if(response.status<500)last.current=null;throw new Error(result.error?.message??'Unable to update the preventive plan.');}
  last.current=null;router.push(`${base}/maintenance/plans/${result.data.id}`);router.refresh();
 }catch(cause){setError(cause instanceof Error?cause.message:'Retry the preventive action.');}finally{setBusy(false);}}
 const canEdit=detail?detail.canWrite&&['draft','paused'].includes(p!.status)&&(p!.created_by===detail.actorId||detail.canReview):Boolean(data?.canCreate);
 const people=detail?.people??data?.people??[];
 const actions:Array<[string,string]>=[];
 if(p&&detail?.canWrite&&p.status!=='cancelled'){
  if(detail.canReview&&['draft','paused'].includes(p.status)&&p.edited_by!==detail.actorId)actions.push(['activate','Review and activate']);
  if(p.status==='active'&&(p.owner_id===detail.actorId||detail.canReview))actions.push(['generate','Generate due work']);
  if(detail.canReview&&p.status==='active')actions.push(['pause','Pause for revision']);
  if(detail.canReview)actions.push(['cancel','Cancel future occurrences']);
 }
 return <>{error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{canEdit&&<FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>{p?'Revise preventive definition':'Create preventive plan'}</summary><FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{
  event.preventDefault();const values=Object.fromEntries(new FormData(event.currentTarget)),asset=data?.assets.find(a=>a.id===values.assetId);
  void send({...values,...(p?{id:p.id,assetId:p.asset_id,expectedVersion:Number(p.version)}:{}),assetVersion:detail?.assetVersion??Number(asset?.version),firstDueAt:new Date(String(values.firstDueAt)).getTime(),intervalDays:Number(values.intervalDays),leadDays:Number(values.leadDays)});
 }}><h2>{p?'Revise preventive definition':'Create preventive plan'}</h2>
  {!p&&<Field appearance="standard">Asset<Select appearance="standard" name="assetId" required>{data?.assets.map(asset=><option key={String(asset.id)} value={String(asset.id)}>{String(asset.code)} · {String(asset.name)}</option>)}</Select></Field>}
  <Field appearance="standard">Inspection or service<TextInput appearance="standard" name="title" defaultValue={p?.title} required minLength={3}/></Field>
  <Field appearance="standard">Priority<Select appearance="standard" name="priority" defaultValue={p?.priority??'normal'}><option value="normal">Normal</option><option value="high">High</option><option value="critical">Critical</option></Select></Field>
  <Field appearance="standard">Assigned owner<Select appearance="standard" name="ownerId" defaultValue={p?.owner_id} required>{people.map(person=><option key={String(person.id)} value={String(person.id)}>{String(person.name)}</option>)}</Select></Field>
  <Field appearance="standard">Repeat every days<TextInput appearance="standard" name="intervalDays" type="number" min="1" max="3650" defaultValue={p?.interval_days??30} required/></Field>
  <PreferredDateTimeField appearance="standard" name="firstDueAt" label={p?'Next first due time':'First due time'} initialInstant={p?Number(p.next_due_at):undefined} required/>
  <Field appearance="standard">Plan time zone<TextInput appearance="standard" name="timeZone" defaultValue={p?.time_zone??'Asia/Bangkok'} required/></Field>
  <Field appearance="standard">Planning window in days<TextInput appearance="standard" name="leadDays" type="number" min="0" max="60" defaultValue={p?.lead_days??7} required/></Field>
  <p className="muted">The schedule keeps its local time. A clock change that removes that time moves that occurrence to the next available minute.</p>
  <Field appearance="standard">Reason<TextArea appearance="standard" name="reason" minLength={3} required/></Field><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>{p?'Save definition':'Create plan'}</Button>
 </FormLayout></FormDisclosure>}
 {p&&actions.length>0&&<FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{event.preventDefault();const values=Object.fromEntries(new FormData(event.currentTarget));void send({...values,id:p.id,expectedVersion:Number(p.version)});}}>
  <h2>Plan action</h2><Field appearance="standard">Action<Select appearance="standard" name="action">
   {actions.map(([value,label])=><option key={value} value={value}>{label}</option>)}
  </Select></Field><Field appearance="standard">Review evidence<TextArea appearance="standard" name="reason" minLength={3} required/></Field><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Record action</Button>
 </FormLayout>}</>;
}
