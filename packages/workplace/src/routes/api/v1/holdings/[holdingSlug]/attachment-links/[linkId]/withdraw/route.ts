import {boundedJsonBody} from '../../../../../../../../lib/bounded-json-body';
import {nativeFieldFamilyCommand} from '../../../../../../../../lib/native-field-family';
import {fieldFileWriterOwner} from '@erpx/server-files/public/field-file-writer-owner';
import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { attachmentLinkStatus, withdrawAttachmentLink } from "@erpx/server-files/public/attachments/service";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
const schema=z.object({reason:z.string().trim().min(3).max(1000)});
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/withdraw">){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");const {holdingSlug,linkId}=await context.params,actor=await requireWorkContext(holdingSlug),body=await boundedJsonBody(request,2*1024*1024),parsed=schema.safeParse(body.value);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"A withdrawal reason is required.");if(await fieldFileWriterOwner(actor,{kind:"link",id:linkId})==="field")return await nativeFieldFamilyCommand(request,actor,"withdraw",linkId,{key,body:new TextDecoder().decode(body.raw)});const {write}=await holdingResources(actor.holdingId);await withdrawAttachmentLink(write,actor,linkId,parsed.data.reason,key);return Response.json(canonicalDestinationLinks({data:await attachmentLinkStatus(write,actor,linkId)}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
