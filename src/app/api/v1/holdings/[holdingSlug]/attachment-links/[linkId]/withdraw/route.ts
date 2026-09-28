import { z } from "zod";
import { attachmentLinkStatus, withdrawAttachmentLink } from "@/server/attachments/service";
import { requireWorkContext } from "@/server/authorization";
import { ApiError,errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
import { holdingResources } from "@/server/runtime";
const schema=z.object({reason:z.string().trim().min(3).max(1000)});
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/withdraw">){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");const {holdingSlug,linkId}=await context.params,actor=await requireWorkContext(holdingSlug),parsed=schema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"A withdrawal reason is required.");const {write}=await holdingResources(actor.holdingId);await withdrawAttachmentLink(write,actor,linkId,parsed.data.reason,key);return Response.json({data:await attachmentLinkStatus(write,actor,linkId)},{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
