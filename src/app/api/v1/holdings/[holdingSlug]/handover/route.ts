import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { handoverView, publishHandover,publishRecoverableHandover } from "@/server/modules/operations/public/handover";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { workSnapshot } from "@/server/modules/work_projection/public/work-items";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };

export async function GET(_request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const [{ write }, snapshot] = await Promise.all([
      holdingResources(actor.holdingId), workSnapshot(actor),
    ]);
    return Response.json(canonicalDestinationLinks({ data: await handoverView(write, actor, snapshot) }), { headers });
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: Context) {
  try {
    if (request.headers.get("origin") !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Publish from this workspace only.");
    const key = request.headers.get("Idempotency-Key");
    if (!key || !/^[a-f0-9-]{36}$/i.test(key))
      throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Provide a valid publish key.");
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const input = await request.json();
    const [{ write }, snapshot] = await Promise.all([
      holdingResources(actor.holdingId), workSnapshot(actor),
    ]);
    return Response.json(canonicalDestinationLinks({ data: await (request.headers.get('x-erpx-handover-publication')==='recoverable'?publishRecoverableHandover(write,actor,snapshot,input,key):publishHandover(write,actor,snapshot,input,key)) }), { headers });
  } catch (error) { return errorResponse(error); }
}
