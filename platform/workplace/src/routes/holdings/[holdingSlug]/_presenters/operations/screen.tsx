import {headers} from 'next/headers';
import {hasModuleAccess} from '@erpx/platform-catalogue/module-access';
import {nativeFieldOperations,nativeActorContext,NativePartsSetupError} from '@erpx/field-service-ui/public/native-parts-setup';
import {validFieldOperationsWorkspace} from '@erpx/operations-ui/lib/operations-work-model';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/identity-access-server/public/authorization";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {nativeOperationsAvailable,nativeOperationsWorkspace,nativeOperationsFailure} from "@erpx/platform-workplace/lib/native-operations";
import {OperationsConsole} from "@erpx/operations-ui/components/operations-console";
import { PageContent, Card } from "@erpx/platform-ui/components/foundation";
export const dynamic="force-dynamic";
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a));
 if(!nativeOperationsAvailable())throw new ApiError(503,'NATIVE_OPERATIONS_UNAVAILABLE','The native Operations reader is unavailable.');
 const data=await nativeOperationsWorkspace(a,holdingSlug).then(data=>{if(!data)throw new ApiError(503,'NATIVE_OPERATIONS_UNAVAILABLE','The native Operations reader is unavailable.');return data;}).catch(error=>{const failure=nativeOperationsFailure(error);return pageError(failure?new ApiError(failure.status,failure.code,failure.message):error,holdingSlug);});
 const field=hasModuleAccess(a.modules,'field_service')?await nativeFieldOperations(await headers(),holdingSlug,nativeActorContext(a),'GET').catch(e=>{console.warn('operations_field_exceptions_unavailable',e instanceof Error?e.message:String(e));return undefined;}):undefined;
 if(field!==undefined&&!validFieldOperationsWorkspace(field,a.companyId))throw new ApiError(503,'NATIVE_FIELD_OPERATIONS_RESPONSE_INVALID','Field operations is unavailable.');
 return <PageContent as="div" className="ui-page-content"><h1>Operations</h1><p>Exceptions from authorized source records, accountable owners and reviewed resolutions.</p><Link href={canonicalUiHref(`${base}/staff-tasks`)}>Tasks and recurring work</Link><Card as="section" appearance="standard" className="ui-panel"><h2>Current queue</h2><p>{data.summary.active} active · {data.summary.awaitingReview} awaiting review · {data.summary.escalated} escalated · {data.summary.overdue} past target</p>{Object.entries(data.summary.byArea).map(([area,count])=><p key={area}>{area} · {count}</p>)}</Card><OperationsConsole base={base} data={data}/><Card as="section" appearance="standard" className="ui-panel"><h2>Exceptions</h2>{data.exceptions.length===0&&<p>No exceptions in your current source scope.</p>}{data.exceptions.map(row=><Link className="ui-record-item" key={row.id} href={canonicalUiHref(`${base}/operations/${row.id}`)}>{row.title} · {row.status} · {row.priority} · {row.source.label}</Link>)}</Card>{field!==undefined&&<section aria-label="Field operational exceptions"><h2>Field operational exceptions</h2><p>{field.summary.active} active · {field.summary.awaitingReview} awaiting independent review</p><OperationsConsole base={base} data={field} commandUrl={`/api/v1${base}/field-service/operations`} sourceKinds={['field_service_order']}/>{field.exceptions.map(row=><Link className="ui-record-item" key={row.id} href={canonicalUiHref(`${base}/field-service/operations/${encodeURIComponent(row.id)}`)}>{row.title} · {row.status} · {row.priority} · {row.source.title}</Link>)}</section>}</PageContent>;
}
