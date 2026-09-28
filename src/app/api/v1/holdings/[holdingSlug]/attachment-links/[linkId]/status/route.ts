import { attachmentLinkStatus } from "@/server/attachments/service";
import { requireWorkContext } from "@/server/authorization";
import { errorResponse } from "@/server/http";
import { holdingResources } from "@/server/runtime";
export async function GET(_request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/status">){try{const {holdingSlug,linkId}=await context.params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);return Response.json({data:await attachmentLinkStatus(write,actor,linkId)},{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
