import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { readDailyPlan, saveDailyPlan } from "@erpx/server-work_projection/public/daily-plan";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { holdingResources } from "../../../../../../../../../packages/server-runtime/src/public/runtime";
import { workSnapshot } from "@erpx/server-work_projection/public/work-items";

type Context = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };

export async function GET(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const date = z.iso.date().parse(new URL(request.url).searchParams.get("date"));
    const [{ write }, snapshot] = await Promise.all([
      holdingResources(actor.holdingId), workSnapshot(actor),
    ]);
    return Response.json(canonicalDestinationLinks({ data: await readDailyPlan(write, actor, date, snapshot) }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request, context: Context) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const commandKey = request.headers.get("Idempotency-Key");
    if (!commandKey) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Provide an Idempotency-Key to save your plan.");
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const input = await request.json();
    const [{ write }, snapshot] = await Promise.all([
      holdingResources(actor.holdingId), workSnapshot(actor),
    ]);
    return Response.json(canonicalDestinationLinks({ data: await saveDailyPlan(write, actor, input, commandKey, snapshot) }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
