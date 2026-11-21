import {headers} from 'next/headers';
import {nativeFieldOperations,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {validFieldOperationsWorkspace} from '@erpx/feature-operations/lib/operations-work-model';
import {OperationsConsole} from '@erpx/feature-operations/components/operations-console';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import WorkspaceLink from '@erpx/workplace/components/workspace-link';
import {ApiError} from '@erpx/server-runtime/public/http';
import {pageError} from '@erpx/server-work_projection/public/page-error';
import {partsSetupContext} from '../../../../../../lib/parts-setup-context';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await nativeFieldOperations(await headers(),holdingSlug,partsSetupContext(actor),'GET').catch(error=>pageError(error instanceof NativePartsSetupError?new ApiError(error.status,error.code,error.message):error,holdingSlug));
 if(!validFieldOperationsWorkspace(data,actor.companyId))throw new ApiError(503,'NATIVE_FIELD_OPERATIONS_RESPONSE_INVALID','Field operations is unavailable.');
 return <section className="ui-page-content"><h1>Field operational exceptions</h1><p>{data.summary.active} active · {data.summary.awaitingReview} awaiting independent review</p><OperationsConsole base={base} data={data} commandUrl={`/api/v1${base}/field-service/operations`} sourceKinds={['field_service_order']}/><section><h2>Exceptions</h2>{data.exceptions.map(row=><p key={row.id}><WorkspaceLink href={canonicalUiHref(`${base}/field-service/operations/${encodeURIComponent(row.id)}`)}>{row.title} · {row.status}</WorkspaceLink></p>)}</section></section>;
}
