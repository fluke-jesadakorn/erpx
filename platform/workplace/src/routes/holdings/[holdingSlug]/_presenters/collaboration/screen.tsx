import {headers} from 'next/headers';
import {nativeFieldCollaboration,nativeActorContext,NativePartsSetupError} from '@erpx/field-service-ui/public/native-parts-setup';
import {validFieldCollaborationReadModel,mergeCollaborationReadModels,type CollaborationWorkReadModel} from '@erpx/collaboration-ui/lib/collaboration-work-read-model';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { collaborationWorkspace } from "@erpx/collaboration-server/public/collaboration-work";
import { CollaborationWorkConsole } from "@erpx/collaboration-ui/components/collaboration-work-console";
import { PageContent } from "@erpx/platform-ui/components/foundation";
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
