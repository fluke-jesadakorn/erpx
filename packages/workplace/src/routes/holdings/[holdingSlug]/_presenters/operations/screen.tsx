import {headers} from 'next/headers';
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {nativeFieldOperations,nativeActorContext,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {validFieldOperationsWorkspace} from '@erpx/feature-operations/lib/operations-work-model';
import {ApiError} from '@erpx/server-runtime/public/http';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {operationsWorkspace} from "@erpx/server-operations/public/operations-exceptions";
import {OperationsConsole} from "@erpx/feature-operations/components/operations-console";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic="force-dynamic";
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,a=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(a)),data=await operationsWorkspace(a).catch(e=>pageError(e,holdingSlug));
 const field=hasModuleAccess(a.modules,'field_service')?await nativeFieldOperations(await headers(),holdingSlug,nativeActorContext(a),'GET').catch(e=>pageError(e instanceof NativePartsSetupError?new ApiError(e.status,e.code,e.message):e,holdingSlug)):undefined;
 if(field!==undefined&&!validFieldOperationsWorkspace(field,a.companyId))throw new ApiError(503,'NATIVE_FIELD_OPERATIONS_RESPONSE_INVALID','Field operations is unavailable.');
 return <PageContent as="div" className="ui-page-content"><h1>Operations</h1><p>Exceptions from authorized source records, accountable owners and reviewed resolutions.</p><Link href={canonicalUiHref(`${base}/staff-tasks`)}>Tasks and recurring work</Link><Card as="section" appearance="standard" className="ui-panel"><h2>Current queue</h2><p>{data.summary.active} active · {data.summary.awaitingReview} awaiting review · {data.summary.escalated} escalated · {data.summary.overdue} past target</p>{Object.entries(data.summary.byArea).map(([area,count])=><p key={area}>{area} · {count}</p>)}</Card><OperationsConsole base={base} data={data}/><Card as="section" appearance="standard" className="ui-panel"><h2>Exceptions</h2>{data.exceptions.length===0&&<p>No exceptions in your current source scope.</p>}{data.exceptions.map(row=><Link className="ui-record-item" key={row.id} href={canonicalUiHref(`${base}/operations/${row.id}`)}>{row.title} · {row.status} · {row.priority} · {row.source.label}</Link>)}</Card>{field!==undefined&&<section aria-label="Field operational exceptions"><h2>Field operational exceptions</h2><p>{field.summary.active} active · {field.summary.awaitingReview} awaiting independent review</p><OperationsConsole base={base} data={field} commandUrl={`/api/v1${base}/field-service/operations`} sourceKinds={['field_service_order']}/>{field.exceptions.map(row=><Link className="ui-record-item" key={row.id} href={canonicalUiHref(`${base}/field-service/operations/${encodeURIComponent(row.id)}`)}>{row.title} · {row.status} · {row.priority} · {row.source.title}</Link>)}</section>}</PageContent>;
}
