import {headers} from 'next/headers';
import {nativeFieldOperations,nativeFieldOperationsDetail,nativeActorContext,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {validFieldOperationsWorkspace,validFieldOperationsDetailRead} from '@erpx/feature-operations/lib/operations-work-model';
import {ApiError} from '@erpx/server-runtime/public/http';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {TimeText} from "@erpx/feature-people_payroll/components/time/time-preference-context";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {operationsExceptionDetail,nativeFieldException} from "@erpx/server-operations/public/operations-exceptions";
import { OperationsConsole } from "@erpx/feature-records/components/loaders/modules--operations-console";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic="force-dynamic";
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a));
 const native=await nativeFieldException(a,id);
 const data=await (async()=>{
  if(!native)return operationsExceptionDetail(a,id);
  const incoming=await headers(),context=nativeActorContext(a);
  const workspace=await nativeFieldOperations(incoming,holdingSlug,context,'GET');
  const record=await nativeFieldOperationsDetail(incoming,holdingSlug,context,id);
  if(!validFieldOperationsWorkspace(workspace,a.companyId)||!validFieldOperationsDetailRead(record,a.companyId,id))throw new ApiError(503,'NATIVE_FIELD_OPERATIONS_RESPONSE_INVALID','Field operations is unavailable.');
  return {...workspace,row:{...record.row,source:record.source},history:record.history,canWrite:record.canWrite,canReview:record.canReview};
 })().catch(e=>pageError(e instanceof NativePartsSetupError?new ApiError(e.status,e.code,e.message):e,holdingSlug));
 const r=data.row,policy=JSON.parse(r.policy_snapshot_json) as {area:string;revision:number;targetMinutes:number};
 return <PageContent as="div" className="ui-page-content"><Link href={canonicalUiHref(`${base}/operations`)}>Operations queue</Link><h1>{r.title}</h1><Card as="section" appearance="standard" className="ui-panel"><p>{r.issue}</p><p>{r.status} · {r.priority} · Target <TimeText appearance="standard" value={r.due_at}/></p><Link href={`${base}${r.source.href}`}>{r.source.title}</Link><p>Recorded policy: {policy.area} revision {policy.revision} · {policy.targetMinutes} minutes</p>{r.resolution&&<p>Proposed resolution: {r.resolution}</p>}</Card><OperationsConsole key={r.version} base={base} detail={data} {...(native?{commandUrl:`/api/v1${base}/field-service/operations`,sourceKinds:['field_service_order']}: {})}/><Card as="section" appearance="standard" className="ui-panel"><h2>History</h2>{data.history.map(event=><p key={String(event.id)}>Version {String(event.version)} · {String(event.action)} · {String(event.reason)} · <TimeText appearance="standard" value={Number(event.created_at)}/></p>)}</Card></PageContent>;
}
