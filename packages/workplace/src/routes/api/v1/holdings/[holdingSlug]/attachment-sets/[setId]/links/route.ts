import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { linkExistingAttachment } from "@erpx/server-files/public/attachments/service";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
import {nativeProjectAttachmentLinksBySet} from "../../../../../../../../../../../packages/feature-projects/src/public/native-project-attachment-read";
import {nativeProjectsAvailable,nativeProjectsFailure,nativeProjectsPageContext} from "../../../../../../../../../../../packages/feature-projects/src/public/native-project-wbs-page";
import {NativeProjectsError} from "../../../../../../../../../../../packages/feature-projects/src/public/native-project-wbs";
const schema=z.object({sourceLinkId:z.string().min(1).max(180),caption:z.string().max(1000).optional()});
function hiddenProjectAttachment(error:unknown){
  return error instanceof NativeProjectsError&&(error.code==="ATTACHMENT_SET_NOT_FOUND"||error.code==="ATTACHMENT_LINK_NOT_FOUND");
}
export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-sets/[setId]/links">){try{const {holdingSlug,setId}=await context.params,actor=await requireWorkContext(holdingSlug);if(nativeProjectsAvailable()){try{const data=await nativeProjectAttachmentLinksBySet(request.headers,nativeProjectsPageContext(actor),setId);return Response.json(canonicalDestinationLinks({data}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){if(!hiddenProjectAttachment(error)){const failure=nativeProjectsFailure(error);if(failure)throw new ApiError(failure.status,failure.code,failure.message);throw error;}}}throw new ApiError(404,"NOT_FOUND","Not found.");}catch(error){return errorResponse(error);}}
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-sets/[setId]/links">){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");const {holdingSlug,setId}=await context.params,actor=await requireWorkContext(holdingSlug),parsed=schema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid source link.");const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await linkExistingAttachment(write,actor,setId,parsed.data,key)}),{status:201});}catch(error){return errorResponse(error);}}
