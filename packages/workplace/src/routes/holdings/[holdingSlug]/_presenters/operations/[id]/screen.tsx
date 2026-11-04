import {headers} from 'next/headers';
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {nativeFieldOperations,nativeFieldOperationsDetail,nativeActorContext,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {validFieldOperationsWorkspace,validFieldOperationsDetailRead} from '@erpx/feature-operations/lib/operations-work-model';
import {ApiError} from '@erpx/server-runtime/public/http';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {TimeText} from "@erpx/feature-people_payroll/components/time/time-preference-context";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {nativeOperationsAvailable,nativeOperationsDetail,nativeOperationsFailure} from "@erpx/workplace/lib/native-operations";
import type {OperationsDetail,OperationsException,OperationsRow,OperationsWorkspace} from "@erpx/feature-operations/lib/operations-work-model";
import { OperationsConsole } from "@erpx/feature-records/components/loaders/modules--operations-console";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic="force-dynamic";
function fieldFailure(error:unknown){return error instanceof NativePartsSetupError?new ApiError(error.status,error.code,error.message):error;}
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a));
 if(!nativeOperationsAvailable())throw new ApiError(503,'NATIVE_OPERATIONS_UNAVAILABLE','The native Operations reader is unavailable.');
 let data:OperationsDetail,field=false;
 try{
  const native=await nativeOperationsDetail(a,holdingSlug,id);
  if(!native)throw new ApiError(503,'NATIVE_OPERATIONS_UNAVAILABLE','The native Operations reader is unavailable.');
  data=native;
 }catch(error){
  const failure=nativeOperationsFailure(error);
  if(failure?.status!==404||!hasModuleAccess(a.modules,'field_service'))await pageError(failure?new ApiError(failure.status,failure.code,failure.message):error,holdingSlug);
  const incoming=await headers(),context=nativeActorContext(a);
  const workspace=await nativeFieldOperations(incoming,holdingSlug,context,'GET').catch(e=>pageError(fieldFailure(e),holdingSlug));
  const record=await nativeFieldOperationsDetail(incoming,holdingSlug,context,id).catch(e=>pageError(fieldFailure(e),holdingSlug));
  if(!validFieldOperationsWorkspace(workspace,a.companyId)||!validFieldOperationsDetailRead(record,a.companyId,id))throw new ApiError(503,'NATIVE_FIELD_OPERATIONS_RESPONSE_INVALID','Field operations is unavailable.');
  const typed=workspace as unknown as OperationsWorkspace,row=record.row as unknown as OperationsException,history=record.history as unknown as OperationsRow[];
  data={...typed,row:{...row,source:record.source as unknown as OperationsException['source']},history} as OperationsDetail;
  field=true;
 }
 const r=data.row,policy=JSON.parse(r.policy_snapshot_json) as {area:string;revision:number;targetMinutes:number};
 return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/operations`)}>Operations queue</Link><h1>{r.title}</h1><Card as="section" appearance="standard" className="ui-panel"><p>{r.issue}</p><p>{r.status} · {r.priority} · Target <TimeText appearance="standard" value={r.due_at}/></p><Link href={`${base}${r.source.href}`}>{r.source.title}</Link><p>Recorded policy: {policy.area} revision {policy.revision} · {policy.targetMinutes} minutes</p>{r.resolution&&<p>Proposed resolution: {r.resolution}</p>}</Card><OperationsConsole key={r.version} base={base} detail={data} {...(field?{commandUrl:`/api/v1${base}/field-service/operations`,sourceKinds:['field_service_order']}: {})}/><Card as="section" appearance="standard" className="ui-panel"><h2>History</h2>{data.history.map(event=><p key={String(event.id)}>Version {String(event.version)} · {String(event.action)} · {String(event.reason)} · <TimeText appearance="standard" value={Number(event.created_at)}/></p>)}</Card></PageContent>;
}
