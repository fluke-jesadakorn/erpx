import {headers} from 'next/headers';
import {nativeFieldOperations,NativePartsSetupError} from '@erpx/field-service-ui/public/native-parts-setup';
import {validFieldOperationsWorkspace} from '@erpx/operations-ui/lib/operations-work-model';
import {OperationsConsole} from '@erpx/operations-ui/components/operations-console';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {canonicalUiHref} from '@erpx/work-projection-ui/lib/destination-registry';
import WorkspaceLink from '@erpx/platform-workplace/components/workspace-link';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {pageError} from '@erpx/work-projection-server/public/page-error';
import {partsSetupContext} from '../../../../../../lib/parts-setup-context';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await nativeFieldOperations(await headers(),holdingSlug,partsSetupContext(actor),'GET').catch(error=>pageError(error instanceof NativePartsSetupError?new ApiError(error.status,error.code,error.message):error,holdingSlug));
 if(!validFieldOperationsWorkspace(data,actor.companyId))throw new ApiError(503,'NATIVE_FIELD_OPERATIONS_RESPONSE_INVALID','Field operations is unavailable.');
 return <section className="ui-page-content"><h1>Field operational exceptions</h1><p>{data.summary.active} active · {data.summary.awaitingReview} awaiting independent review</p><OperationsConsole base={base} data={data} commandUrl={`/api/v1${base}/field-service/operations`} sourceKinds={['field_service_order']}/><section><h2>Exceptions</h2>{data.exceptions.map(row=><p key={row.id}><WorkspaceLink href={canonicalUiHref(`${base}/field-service/operations/${encodeURIComponent(row.id)}`)}>{row.title} · {row.status}</WorkspaceLink></p>)}</section></section>;
}
