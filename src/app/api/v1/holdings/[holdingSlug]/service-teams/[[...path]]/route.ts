import { serviceTeamOptions } from "@/server/service-team-options";
import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { ApiError, errorResponse } from "@/server/http";
import { serviceCommandBody } from "@/server/service-http";
import { requireServiceOwner } from "@/server/service-access";
import {
  saveServiceTeam,
  serviceTeamSchema,
} from "@/server/service-teams";
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
      { data: await serviceTeamOptions(write, actor) },
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
    return Response.json({
      data: await saveServiceTeam(
        write,
        actor,
        path[0] ?? null,
        serviceTeamSchema.parse(input),
        key,
      ),
    });
  } catch (e) {
    return errorResponse(e);
  }
}
