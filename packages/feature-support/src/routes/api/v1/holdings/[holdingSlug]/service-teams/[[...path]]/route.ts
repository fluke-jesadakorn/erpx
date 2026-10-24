import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { serviceTeamOptions } from "@erpx/server-support/public/service-team-options";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { serviceCommandBody } from "@erpx/server-support/public/service-http";
import { requireServiceOwner } from "@erpx/server-support/public/service-access";
import {saveServiceTeam,serviceTeamSchema} from '@erpx/server-support/public/service-teams';
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(_request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(actor.holdingId);
    await requireServiceOwner(write, actor);
    if (path.length)
      throw new ApiError(404, "NOT_FOUND", "Team route not found.");
    return Response.json(
      canonicalDestinationLinks({ data: await serviceTeamOptions(write, actor) }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(actor.holdingId),
      { key, input } = await serviceCommandBody(request);
    if (path.length > 1)
      throw new ApiError(404, "NOT_FOUND", "Team route not found.");
    return Response.json(canonicalDestinationLinks({
      data: await saveServiceTeam(
        write,
        actor,
        path[0] ?? null,
        serviceTeamSchema.parse(input),
        key,
      ),
    }));
  } catch (e) {
    return errorResponse(e);
  }
}
