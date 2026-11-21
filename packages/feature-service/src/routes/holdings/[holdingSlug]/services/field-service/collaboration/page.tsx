import {headers} from 'next/headers';
import {nativeFieldCollaboration,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {validFieldCollaborationReadModel} from '@erpx/feature-collaboration/lib/collaboration-work-read-model';
import {CollaborationWorkConsole} from '@erpx/feature-collaboration/components/collaboration-work-console';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {ApiError} from '@erpx/server-runtime/public/http';
import {pageError} from '@erpx/server-work_projection/public/page-error';
import {partsSetupContext} from '../../../../../../lib/parts-setup-context';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await nativeFieldCollaboration(await headers(),holdingSlug,partsSetupContext(actor),'GET').catch(error=>pageError(error instanceof NativePartsSetupError?new ApiError(error.status,error.code,error.message):error,holdingSlug));
 if(!validFieldCollaborationReadModel(data))throw new ApiError(503,'NATIVE_FIELD_COLLABORATION_RESPONSE_INVALID','Field collaboration is unavailable.');
 return <section className="ui-page-content"><h1>Field collaboration</h1><p>Link Service orders and route accountable actions with their authorized source records.</p><CollaborationWorkConsole base={base} data={data} commandUrl={`/api/v1${base}/field-service/collaboration`} exportHref={null}/></section>;
}
