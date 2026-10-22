import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { approveAttachmentPublication } from "@erpx/server-files/public/attachments/service";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/media-publications/[publicationId]/approve">){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","A valid Idempotency-Key is required.");const {holdingSlug,publicationId}=await context.params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await approveAttachmentPublication(write,actor,publicationId,key)}));}catch(error){return errorResponse(error);}}
