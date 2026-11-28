import type { RouteContext } from "@erpx/platform-contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { z } from "zod";
import { revokeAttachmentPublication } from "@erpx/files-server/public/attachments/service";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError,errorResponse } from "@erpx/platform-runtime-server/public/http";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
import { holdingResources } from "../../../../../../../../../../runtime/server/src/public/runtime";
const schema=z.object({reason:z.string().trim().min(3).max(1000)});
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/media-publications/[publicationId]/revoke">){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");const {holdingSlug,publicationId}=await context.params,actor=await requireWorkContext(holdingSlug),parsed=schema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"A revocation reason is required.");const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await revokeAttachmentPublication(write,actor,publicationId,parsed.data.reason,key)}));}catch(error){return errorResponse(error);}}
