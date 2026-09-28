import { requestedHoldingScope, requireWorkContext } from "@/server/authorization";
import { readSeatForecastBaseline } from "@/server/billing-forecast";
import { ApiError, errorResponse } from "@/server/http";
import { holdingResources, runtimeEnv } from "@/server/runtime";
import type { PostgresDatabase } from "@/server/runtime/postgres";
import { requireCompanyAuthority } from "@/server/staff-policy";

export async function GET(_request: Request, context: { params: Promise<{ holdingSlug: string }> }) {
  try {
    const { holdingSlug } = await context.params;
    const requested = await requestedHoldingScope(holdingSlug);
    if (requested.kind !== "holding")
      throw new ApiError(403, "FORBIDDEN", "The forecast is available in the Holding workspace.");
    const actor = await requireWorkContext(requested);
    if (actor.role !== "holding_owner" || actor.scope.kind !== "holding")
      throw new ApiError(403, "FORBIDDEN", "Only the Holding owner can view this forecast.");
    await requireCompanyAuthority((await holdingResources(actor.holdingId)).write, actor, ["holding_owner"]);
    const db = (await runtimeEnv()).CONTROL_DB as PostgresDatabase;
    return Response.json({ data: await readSeatForecastBaseline(db, actor.holdingId) },
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
