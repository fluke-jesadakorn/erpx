import {headers} from 'next/headers';
import {nativeFieldOperations,nativeFieldOperationsDetail,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {validFieldOperationsWorkspace,validFieldOperationsDetailRead} from '@erpx/feature-operations/lib/operations-work-model';
import {OperationsConsole} from '@erpx/feature-operations/components/operations-console';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import WorkspaceLink from '@erpx/workplace/components/workspace-link';
import {ApiError} from '@erpx/server-runtime/public/http';
import {pageError} from '@erpx/server-work_projection/public/page-error';
import {partsSetupContext} from '../../../../../../../lib/parts-setup-context';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await nativeFieldOperations(await headers(),holdingSlug,partsSetupContext(actor),'GET').catch(error=>pageError(error instanceof NativePartsSetupError?new ApiError(error.status,error.code,error.message):error,holdingSlug));
 if(!validFieldOperationsWorkspace(data,actor.companyId))throw new ApiError(503,'NATIVE_FIELD_OPERATIONS_RESPONSE_INVALID','Field operations is unavailable.');
 const record=await nativeFieldOperationsDetail(await headers(),holdingSlug,partsSetupContext(actor),id).catch(error=>pageError(error instanceof NativePartsSetupError?new ApiError(error.status,error.code,error.message):error,holdingSlug));
 if(!validFieldOperationsDetailRead(record,actor.companyId,id))throw new ApiError(503,'NATIVE_FIELD_OPERATIONS_RESPONSE_INVALID','Field operations is unavailable.');
 const row={...record.row,source:record.source},detail={...data,row,history:record.history};
 return <section className="ui-page-content"><WorkspaceLink href={canonicalUiHref(`${base}/field-service/operations`)}>Field operational exceptions</WorkspaceLink><h1>{row.title}</h1><p>{row.status} · {row.priority} · Version {row.version}</p><p style={{whiteSpace:'pre-wrap',overflowWrap:'anywhere'}}>{row.issue}</p><OperationsConsole base={base} detail={detail} commandUrl={`/api/v1${base}/field-service/operations`} sourceKinds={['field_service_order']}/><section><h2>History</h2><ol>{detail.history.map(event=><li key={String(event.id)}>{String(event.action)} · Version {String(event.version)} · {String(event.reason)}</li>)}</ol></section></section>;
}
