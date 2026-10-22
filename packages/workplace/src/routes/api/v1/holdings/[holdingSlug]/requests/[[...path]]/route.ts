import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {
  requestAiStatus,
  queueRequestAi,
  runRequestAi,
  requestAiInput,
} from "@erpx/server-records/public/request-ai";
import { isThaiHotelAiMockEnabled } from "@erpx/server-runtime/public/simulation-only-adapters";
import { runtimeEnv } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { serviceCommandBody } from "@erpx/server-support/public/service-http";
import {
  employeeRequestDetail,
  listEmployeeRequests,
  saveEmployeeRequest,
  actOnEmployeeRequest,
  employeeRequestSchema,
  requestActionSchema,
} from "@erpx/server-records/public/employee-requests";
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
