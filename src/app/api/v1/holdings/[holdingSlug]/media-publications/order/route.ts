import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { z } from "zod";
import { reorderAttachmentPublications } from "@/server/modules/files/public/attachments/service";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError,errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
const schema=z.object({targetType:z.string().min(1).max(80),targetId:z.string().min(1).max(180),expectedVersion:z.number().int().positive(),items:z.array(z.object({id:z.string().min(1).max(180),ordinal:z.number().int().min(0).max(10000)})).min(1).max(100)});
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/media-publications/order">){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");const {holdingSlug}=await context.params,actor=await requireWorkContext(holdingSlug),parsed=schema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid publication order.");const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await reorderAttachmentPublications(write,actor,parsed.data,key)}));}catch(error){return errorResponse(error);}}
