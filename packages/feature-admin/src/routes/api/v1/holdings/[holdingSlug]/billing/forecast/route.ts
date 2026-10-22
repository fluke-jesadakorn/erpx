import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requestedHoldingScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { readSeatForecastBaseline } from "@erpx/server-billing/public/billing-forecast";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { holdingResources, runtimeEnv } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { requireCompanyAuthority } from "@erpx/server-operations/public/staff-policy";

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
    return Response.json(canonicalDestinationLinks({ data: await readSeatForecastBaseline(db, actor.holdingId) }),
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
