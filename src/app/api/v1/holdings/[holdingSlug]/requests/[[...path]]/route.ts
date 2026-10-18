import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import {
  requestAiStatus,
  queueRequestAi,
  runRequestAi,
  requestAiInput,
} from "@/server/modules/records/public/request-ai";
import { isThaiHotelAiMockEnabled } from "@/server/modules/runtime/public/simulation-only-adapters";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { serviceCommandBody } from "@/server/modules/support/public/service-http";
import {
  employeeRequestDetail,
  listEmployeeRequests,
  saveEmployeeRequest,
  actOnEmployeeRequest,
  employeeRequestSchema,
  requestActionSchema,
} from "@/server/modules/records/public/employee-requests";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(_request: Request, context: Context) {
  try {
    const { holdingSlug, path = [] } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(actor.holdingId);
    if (path.length === 2 && path[1] === "ai")
      return Response.json(
        canonicalDestinationLinks({
          data: await requestAiStatus(
            write,
            actor,
            await runtimeEnv(),
            path[0],
          ),
        }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path.length > 1)
      throw new ApiError(404, "NOT_FOUND", "Request route not found.");
    return Response.json(
      canonicalDestinationLinks({
        data: path.length
          ? await employeeRequestDetail(write, actor, path[0])
          : await listEmployeeRequests(write, actor),
      }),
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
    let data;
    if (path.length === 0)
      data = await saveEmployeeRequest(
        write,
        actor,
        null,
        employeeRequestSchema.parse(input),
        key,
      );
    else if (path.length === 1)
      data = await saveEmployeeRequest(
        write,
        actor,
        path[0],
        employeeRequestSchema.parse(input),
        key,
      );
    else if (path.length === 2 && path[1] === "ai") {
      const env = await runtimeEnv();
      data = await queueRequestAi(
        write,
        actor,
        env,
        path[0],
        requestAiInput.parse(input),
        key,
      );
      // The simulation runs the same committed worker handler inline so browser
      // acceptance does not depend on a separately running jobs process.
      if (isThaiHotelAiMockEnabled(holdingSlug))
        await runRequestAi(write, env, data.proposalId);
    } else if (path.length === 2 && path[1] === "actions")
      data = await actOnEmployeeRequest(
        write,
        actor,
        path[0],
        requestActionSchema.parse(input),
        key,
      );
    else throw new ApiError(404, "NOT_FOUND", "Request action not found.");
    return Response.json(canonicalDestinationLinks({ data }));
  } catch (e) {
    return errorResponse(e);
  }
}
