import type { RouteContext } from "@erpx/contracts/next-routes";
import {nativeFieldAttachment} from "@erpx/server-files/public/attachments/native-field-attachment";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { attachmentLinkStatus, withdrawAttachmentLink } from "@erpx/server-files/public/attachments/service";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
const schema=z.object({reason:z.string().trim().min(3).max(1000)});
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/withdraw">){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");const {holdingSlug,linkId}=await context.params,actor=await requireWorkContext(holdingSlug),original=request.clone(),parsed=schema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"A withdrawal reason is required.");const native=await nativeFieldAttachment(request.headers,actor,linkId,{key,body:await original.text()});if(native)return Response.json(canonicalDestinationLinks({data:native}),{headers:{"Cache-Control":"private, no-store"}});const {write}=await holdingResources(actor.holdingId);await withdrawAttachmentLink(write,actor,linkId,parsed.data.reason,key);return Response.json(canonicalDestinationLinks({data:await attachmentLinkStatus(write,actor,linkId)}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
