import { attachmentLinkStatus } from "@/server/modules/files/public/attachments/service";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse } from "@/server/modules/runtime/public/http";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
export async function GET(_request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/status">){try{const {holdingSlug,linkId}=await context.params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);return Response.json({data:await attachmentLinkStatus(write,actor,linkId)},{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
