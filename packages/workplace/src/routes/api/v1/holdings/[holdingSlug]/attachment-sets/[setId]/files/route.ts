import {boundedJsonBody} from '../../../../../../../../lib/bounded-json-body';
import {nativeFieldFamily} from '../../../../../../../../lib/native-field-family';
import {fieldFileWriterOwner} from '@erpx/server-files/public/field-file-writer-owner';
import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { reserveAttachmentFile } from "@erpx/server-files/public/attachments/service";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
const schema=z.object({filename:z.string().min(1).max(255),mediaType:z.string().min(1).max(100),bytes:z.number().int().positive().max(10*1024*1024),caption:z.string().max(1000).optional(),supersedesLinkId:z.string().min(1).max(180).optional()});
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-sets/[setId]/files">){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");const {holdingSlug,setId}=await context.params,actor=await requireWorkContext(holdingSlug),body=await boundedJsonBody(request,64*1024),parsed=schema.safeParse(body.value);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid file reservation.");if(await fieldFileWriterOwner(actor,{kind:"set",id:setId})==="field")return await nativeFieldFamily(request,actor,{route:{kind:"reserve",setId},body:body.raw,contentType:"application/json",key});const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await reserveAttachmentFile(write,actor,setId,parsed.data,key)}),{status:201});}catch(error){return errorResponse(error);}}
