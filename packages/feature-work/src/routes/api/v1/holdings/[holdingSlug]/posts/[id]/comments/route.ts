import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { postComments } from "@erpx/server-collaboration/public/social-home";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { createComment, createCommentSchema } from "@erpx/server-collaboration/public/social";

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/posts/[id]/comments">) {
  try {
    const { holdingSlug, id } = await context.params;
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const parsed = createCommentSchema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid comment.");
    const actor = await requireWorkContext(holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await createComment(actor.holdingId, id, parsed.data, commandId, actor) }), { status: 201 });
  } catch (error) { return errorResponse(error); }
}

export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/posts/[id]/comments">){
  try {
    const {holdingSlug,id}=await context.params,actor=await requireWorkContext(holdingSlug);
    const {write}=await holdingResources(actor.holdingId);
    return Response.json(canonicalDestinationLinks({data:await postComments(write,actor,id,new URL(request.url).searchParams.get("cursor")??undefined)}),{headers:{"Cache-Control":"private, no-store"}});
  } catch(error){return errorResponse(error);}
}
