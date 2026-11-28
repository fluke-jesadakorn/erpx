import {nativeFieldCollaboration,nativeActorContext,NativePartsSetupError} from '@erpx/field-service-ui/public/native-parts-setup';
import {validFieldCollaborationReadModel,mergeCollaborationReadModels,type CollaborationWorkReadModel} from '@erpx/collaboration-ui/lib/collaboration-work-read-model';
import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
import { ApiError, errorResponse, boundedCommandBody } from "@erpx/platform-runtime-server/public/http";
import {
  nativeFieldCollaborationCommand,
  collaborationExport,
} from "@erpx/collaboration-server/public/collaboration-work";
import { nativeCollaborationCommand, nativeCollaborationWorkspace } from "@erpx/collaboration-server/public/native-collaboration-page";
function failure(e:unknown){return errorResponse(e instanceof NativePartsSetupError?new ApiError(e.status,e.code,e.message):e);}
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug);
    if (new URL(r.url).searchParams.get("format") === "csv") {
      if(a.modules.includes('field_service'))throw new ApiError(503,'NATIVE_FIELD_EXPORT_UNAVAILABLE','A complete native collaboration export is not available.');
      // Retained CSV presentation reader: the native workspace port answers JSON
      // only. This branch is a separate export format, not a workspace fallback.
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
    const native=await nativeCollaborationWorkspace(r,(await c.params).holdingSlug);
    if(!native)throw new ApiError(503,'NATIVE_COLLABORATION_RESPONSE_INVALID','Collaboration workspace unavailable.');
    let data:CollaborationWorkReadModel={
      relations:(native.relations??[]) as CollaborationWorkReadModel['relations'],
      cards:(native.cards??[]) as CollaborationWorkReadModel['cards'],
      people:(native.people??[]) as CollaborationWorkReadModel['people'],
      sourceTypes:(native.sourceTypes??[]) as CollaborationWorkReadModel['sourceTypes'],
      canWrite:native.access?.canWrite===true,
      canReview:native.access?.canReview===true,
    };
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
    const data=await nativeCollaborationCommand(r,(await c.params).holdingSlug,body,key);
    return Response.json(canonicalDestinationLinks({data}),{headers});
  } catch (e) {
    return failure(e);
  }
}
