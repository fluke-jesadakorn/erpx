"use client";
import WorkspaceLink from "@erpx/workplace/components/workspace-link";
import { InlineNotice } from "@erpx/ui/components/foundation";

import {AssetEditor,AssetRecoveryBoundary} from "@erpx/feature-assets/components/asset-editor";
import {useAssetCommand} from "@erpx/feature-assets/components/use-asset-command";
import {assetCommandAllowed} from "@erpx/feature-assets/lib/command";
import {Button,Field,Select,TextArea,TextInput} from "@erpx/ui/components/foundation";
import {PreferredDateTimeField} from "@erpx/feature-people_payroll/components/time/preferred-date-time-field";
import type {maintenanceWorkspace} from "@erpx/server-maintenance/public/asset-maintenance";
import type {preventivePlanDetail} from "@erpx/server-maintenance/public/maintenance-preventive";
import { FormLayout } from "@erpx/ui/components/foundation";

export function MaintenancePreventiveConsole({base,data,detail}:{base:string;data?:Awaited<ReturnType<typeof maintenanceWorkspace>>;detail?:Awaited<ReturnType<typeof preventivePlanDetail>>}){
 const purchases=detail?.purchases??data?.purchases,{send,retry,busy,error,notice,saved,uncertain}=useAssetCommand(base,"maintenance/plans",purchases,detail?.plan),p=detail?.plan;
 const canEdit=detail?detail.canWrite&&['draft','paused'].includes(p!.status)&&(p!.created_by===detail.actorId||detail.canReview):Boolean(data?.canCreate);
 const people=detail?.people??data?.people??[];
 const actions:Array<[string,string]>=[];
 if(p&&detail?.canWrite&&p.status!=='cancelled'){
  if(detail.canReview&&['draft','paused'].includes(p.status)&&p.edited_by!==detail.actorId)actions.push(['activate','Review and activate']);
  if(p.status==='active'&&(p.owner_id===detail.actorId||detail.canReview))actions.push(['generate','Generate due work']);
  if(detail.canReview&&p.status==='active')actions.push(['pause','Pause for revision']);
  if(detail.canReview)actions.push(['cancel','Cancel future occurrences']);
 }
 return <AssetRecoveryBoundary recovery={{retry,busy,uncertain,notice,saved}}>{error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{notice&&<InlineNotice role="status">{notice}</InlineNotice>}{saved&&<p>Saved original: <WorkspaceLink href={saved.href}>{saved.id}</WorkspaceLink></p>}{uncertain&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>void retry()}>Retry submitted change</Button>}<AssetEditor title={p?"Revise preventive definition":"Create preventive plan"} busy={busy} error={error} visible={canEdit} disabled={!assetCommandAllowed("maintenance/plans",{},purchases)}><FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{
  event.preventDefault();const values=Object.fromEntries(new FormData(event.currentTarget)),asset=data?.assets.find(a=>a.id===values.assetId);
  void send({...values,...(p?{id:p.id,assetId:p.asset_id,expectedVersion:Number(p.version)}:{}),assetVersion:detail?.assetVersion??Number(asset?.version),firstDueAt:new Date(String(values.firstDueAt)).getTime(),intervalDays:Number(values.intervalDays),leadDays:Number(values.leadDays)},event.currentTarget);
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
 </FormLayout></AssetEditor>
 {p&&<AssetEditor title="Plan action" busy={busy} error={error} visible={actions.length>0} disabled={actions.every(([action])=>!assetCommandAllowed("maintenance/plans",{id:p.id,action},purchases))}><FormLayout className="ui-panel ui-form-grid" onSubmit={event=>{event.preventDefault();const values=Object.fromEntries(new FormData(event.currentTarget));void send({...values,id:p.id,expectedVersion:Number(p.version)},event.currentTarget);}}>
  <h2>Plan action</h2><Field appearance="standard">Action<Select appearance="standard" name="action">
   {actions.map(([value,label])=><option key={value} value={value} disabled={!assetCommandAllowed("maintenance/plans",{id:p!.id,action:value},purchases)}>{label}</option>)}
  </Select></Field><Field appearance="standard">Review evidence<TextArea appearance="standard" name="reason" minLength={3} required/></Field><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Record action</Button>
 </FormLayout></AssetEditor>}</AssetRecoveryBoundary>;
}
