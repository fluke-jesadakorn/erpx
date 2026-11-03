import {headers} from 'next/headers';
import {nativeFieldCollaboration,nativeActorContext,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {validFieldCollaborationReadModel,mergeCollaborationReadModels,type CollaborationWorkReadModel} from '@erpx/feature-collaboration/lib/collaboration-work-read-model';
import {ApiError} from '@erpx/server-runtime/public/http';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { collaborationWorkspace } from "@erpx/server-collaboration/public/collaboration-work";
import { CollaborationWorkConsole } from "@erpx/feature-collaboration/components/collaboration-work-console";
import { PageContent } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    legacy = await collaborationWorkspace(a).catch((e) =>pageError(e,holdingSlug));
  let data:CollaborationWorkReadModel=legacy;
  if(a.modules.includes('field_service')){
   const native=await nativeFieldCollaboration(await headers(),holdingSlug,nativeActorContext(a),'GET').catch(e=>pageError(e instanceof NativePartsSetupError?new ApiError(e.status,e.code,e.message):e,holdingSlug));
   if(!validFieldCollaborationReadModel(native))throw new ApiError(503,'NATIVE_FIELD_COLLABORATION_RESPONSE_INVALID','Field collaboration is unavailable.');
   data=mergeCollaborationReadModels(legacy,native);
  }
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/work`)}>Work</Link>
      <h1>Cross-record collaboration</h1>
      <p>
        Keep typed backlinks, accountable next actions and authorized
        conversation files connected to their source records.
      </p>
      <CollaborationWorkConsole base={base} data={data} exportHref={a.modules.includes('field_service')?null:undefined} />
    </PageContent>
  );
}
