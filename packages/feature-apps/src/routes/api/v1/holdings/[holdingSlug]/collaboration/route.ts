import {nativeFieldCollaboration,nativeActorContext,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {validFieldCollaborationReadModel,mergeCollaborationReadModels,type CollaborationWorkReadModel} from '@erpx/feature-collaboration/lib/collaboration-work-read-model';
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse, boundedCommandBody } from "@erpx/server-runtime/public/http";
import {
  nativeFieldCollaborationCommand,
  changeActionCard,
  collaborationExport,
  collaborationWorkspace,
  createActionCard,
  createRecordRelation,
  removeRecordRelation,
} from "@erpx/server-collaboration/public/collaboration-work";
import { nativeCollaborationAvailable, nativeCollaborationWorkspace } from "@erpx/server-collaboration/public/native-collaboration-page";
function failure(e:unknown){return errorResponse(e instanceof NativePartsSetupError?new ApiError(e.status,e.code,e.message):e);}
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug);
    if (new URL(r.url).searchParams.get("format") === "csv") {
      if(a.modules.includes('field_service'))throw new ApiError(503,'NATIVE_FIELD_EXPORT_UNAVAILABLE','A complete native collaboration export is not available.');
      const result = await collaborationExport(a);
      return new Response(result.content, {
        headers: {
          "Content-Type": "text/csv; charset=utf-8",
          "Content-Disposition": 'attachment; filename="erpx-collaboration.csv"',
          "Cache-Control": "private, no-store",
          ETag: result.sha256,
        },
      });
    }
    let data:CollaborationWorkReadModel=await collaborationWorkspace(a);
    if(nativeCollaborationAvailable()){
      const native=await nativeCollaborationWorkspace(r,(await c.params).holdingSlug);
      if(!native)throw new ApiError(503,'NATIVE_COLLABORATION_RESPONSE_INVALID','Collaboration workspace unavailable.');
      data={
        relations:(native.relations??[]) as CollaborationWorkReadModel['relations'],
        cards:(native.cards??[]) as CollaborationWorkReadModel['cards'],
        people:(native.people??[]) as CollaborationWorkReadModel['people'],
        sourceTypes:(native.sourceTypes??[]) as CollaborationWorkReadModel['sourceTypes'],
        canWrite:native.access?.canWrite===true,
        canReview:native.access?.canReview===true,
      };
    }
    if(a.modules.includes('field_service')){
      const native=await nativeFieldCollaboration(r.headers,a.holdingSlug,nativeActorContext(a),'GET');
      if(!validFieldCollaborationReadModel(native))throw new ApiError(503,'NATIVE_FIELD_COLLABORATION_RESPONSE_INVALID','Field collaboration is unavailable.');
      data=mergeCollaborationReadModels(data,native);
    }
    return Response.json(
      canonicalDestinationLinks({data}),
      { headers },
    );
  } catch (e) {
    return failure(e);
  }
}
export async function POST(r: Request, c: C) {
  try {
    if (
      r.headers.get("origin") &&
      r.headers.get("origin") !== requestPublicOrigin(r)
    )
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const key = r.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const a = await requireWorkContext((await c.params).holdingSlug),body=await boundedCommandBody(r),input:unknown=JSON.parse(body);
    if(await nativeFieldCollaborationCommand(a,input)){
      if(r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
      const data=await nativeFieldCollaboration(r.headers,a.holdingSlug,nativeActorContext(a),'POST',body,key);
      return Response.json(canonicalDestinationLinks({data}),{headers});
    }
    const legacyInput=input as Record<string,unknown>;
    const removeId=typeof legacyInput.id==='string'?legacyInput.id:undefined;
    if(legacyInput.entity==='relation'&&legacyInput.action==='remove'&&removeId===undefined)throw new ApiError(422,'VALIDATION_FAILED','Choose a relation to remove.');
    const data =
      legacyInput.entity === "relation"
        ? legacyInput.action === "remove"
          ? await removeRecordRelation(
              a,
              removeId!,
              Number(legacyInput.expectedVersion),
              String(legacyInput.reason),
              key,
            )
          : await createRecordRelation(a, legacyInput, key)
        : typeof legacyInput.id === "string"
          ? await changeActionCard(a, legacyInput.id, legacyInput, key)
          : await createActionCard(a, legacyInput, key);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return failure(e);
  }
}
