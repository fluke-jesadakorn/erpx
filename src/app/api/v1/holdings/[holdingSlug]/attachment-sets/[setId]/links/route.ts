import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { z } from "zod";
import { linkExistingAttachment } from "@/server/modules/files/public/attachments/service";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError,errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
const schema=z.object({sourceLinkId:z.string().min(1).max(180),caption:z.string().max(1000).optional()});
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-sets/[setId]/links">){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");const {holdingSlug,setId}=await context.params,actor=await requireWorkContext(holdingSlug),parsed=schema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid source link.");const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await linkExistingAttachment(write,actor,setId,parsed.data,key)}),{status:201});}catch(error){return errorResponse(error);}}
