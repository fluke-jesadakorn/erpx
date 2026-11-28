import {headers} from 'next/headers';
import {nativeFieldCollaboration,NativePartsSetupError} from '@erpx/field-service-ui/public/native-parts-setup';
import {validFieldCollaborationReadModel} from '@erpx/collaboration-ui/lib/collaboration-work-read-model';
import {CollaborationWorkConsole} from '@erpx/collaboration-ui/components/collaboration-work-console';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {pageError} from '@erpx/work-projection-server/public/page-error';
import {partsSetupContext} from '../../../../../../lib/parts-setup-context';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await nativeFieldCollaboration(await headers(),holdingSlug,partsSetupContext(actor),'GET').catch(error=>pageError(error instanceof NativePartsSetupError?new ApiError(error.status,error.code,error.message):error,holdingSlug));
 if(!validFieldCollaborationReadModel(data))throw new ApiError(503,'NATIVE_FIELD_COLLABORATION_RESPONSE_INVALID','Field collaboration is unavailable.');
 return <section className="ui-page-content"><h1>Field collaboration</h1><p>Link Service orders and route accountable actions with their authorized source records.</p><CollaborationWorkConsole base={base} data={data} commandUrl={`/api/v1${base}/field-service/collaboration`} exportHref={null}/></section>;
}
