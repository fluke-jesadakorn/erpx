import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../../../../../../../../platform/runtime/server/src/public/runtime";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
import { releaseAcceptance, submitReleaseAcceptance, reviewReleaseAcceptance, revokeReleaseAcceptance,bindReleaseEvidence } from "@erpx/platform-administration-server/public/release-acceptance";

type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const headers = { 'Cache-Control': 'private, no-store' };

export async function GET(_request: Request, { params }: Context) {
  try {
    const { holdingSlug, path = [] } = await params;
    if (path.length > 1) throw new ApiError(404, 'NOT_FOUND', 'Release route not found.');
    const actor = await requireWorkContext(holdingSlug);
    const { write } = await holdingResources(actor.holdingId);
    return Response.json(canonicalDestinationLinks({ data: await releaseAcceptance(write, actor, path[0]) }), { headers });
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, { params }: Context) {
  try {
    if (request.headers.get('origin') !== requestPublicOrigin(request)) throw new ApiError(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
    const key = request.headers.get('Idempotency-Key');
    if (!key) throw new ApiError(422, 'COMMAND_KEY_REQUIRED', 'Idempotency-Key is required.');
    const { holdingSlug, path = [] } = await params;
    const actor = await requireWorkContext(holdingSlug);
    const { write } = await holdingResources(actor.holdingId);
    const input = await request.json();
    const data = path.length === 0 ? await submitReleaseAcceptance(write, actor, input, key)
      : path.length === 2 && path[1] === 'review' ? await reviewReleaseAcceptance(write, actor, path[0], input, key)
        : path.length === 2 && path[1] === 'revoke' ? await revokeReleaseAcceptance(write, actor, path[0], input, key)
          : path.length===2&&path[1]==='evidence'?await bindReleaseEvidence(write,actor,path[0],input,key)
          : null;
    if (!data) throw new ApiError(404, 'NOT_FOUND', 'Release route not found.');
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) { return errorResponse(error); }
}
