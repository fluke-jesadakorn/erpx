import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { billingMonth } from "@erpx/feature-billing/lib/billing";
import { requestedHoldingScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { readTargetBillingPeriod } from "@erpx/server-billing/public/billing-target";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { holdingResources, runtimeEnv } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { requireCompanyAuthority } from "@erpx/server-operations/public/staff-policy";

export async function GET(request: Request, context: { params: Promise<{ holdingSlug: string }> }) {
  try {
    const { holdingSlug } = await context.params;
    const requested = await requestedHoldingScope(holdingSlug);
    if (requested.kind !== "holding")
      throw new ApiError(403, "FORBIDDEN", "Target billing is available in the Holding workspace.");
    const actor = await requireWorkContext(requested);
    if (actor.role !== "holding_owner" || actor.scope.kind !== "holding")
      throw new ApiError(403, "FORBIDDEN", "Only the Holding owner can view target billing.");
    await requireCompanyAuthority((await holdingResources(actor.holdingId)).write, actor, ["holding_owner"]);
    const month = new URL(request.url).searchParams.get("month") ?? billingMonth(Date.now());
    if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(month))
      throw new ApiError(422, "INVALID_MONTH", "Use a month in YYYY-MM format.");
    const db = (await runtimeEnv()).CONTROL_DB as PostgresDatabase;
    const view = await readTargetBillingPeriod(db, actor.holdingId, month);
    return Response.json(canonicalDestinationLinks({ data: {
      holdingId: view.holdingId,
      month: view.month,
      assignment: view.assignment,
      price: view.price,
      measuredEvidence: {
        snapshot: view.measuredEvidence.snapshot && {
          refreshedAt: view.measuredEvidence.snapshot.refreshedAt,
          sourceCount: view.measuredEvidence.snapshot.sourceCount,
          seatSourceCount: view.measuredEvidence.snapshot.seatSourceCount,
          aiCoverage: view.measuredEvidence.snapshot.aiCoverage,
          storageCoverage: view.measuredEvidence.snapshot.storageCoverage,
          seatCoverage: view.measuredEvidence.snapshot.seatCoverage,
        },
        ai: {
          requests: view.measuredEvidence.ai.requests,
          inputTokens: view.measuredEvidence.ai.inputTokens,
          outputTokens: view.measuredEvidence.ai.outputTokens,
          platformSucceeded: view.measuredEvidence.ai.platformSucceeded,
          platformVerified: view.measuredEvidence.ai.platformVerified,
          platformPending: view.measuredEvidence.ai.platformPending,
          byok: view.measuredEvidence.ai.byok,
          failed: view.measuredEvidence.ai.failed,
          simulation: view.measuredEvidence.ai.simulation,
          unknown: view.measuredEvidence.ai.unknown,
        },
        storage: view.measuredEvidence.storage,
        seats: view.measuredEvidence.seats,
      },
      completeness: { ...view.completeness, missing: [...new Set(view.completeness.missing.map((reason) => reason.split(":")[0]))] },
      measuredSourceCount: view.measuredEvidence.snapshot?.sourceCount ?? null,
      verification: {
        required: view.sources.filter((line) => line.kind === "ai" && line.reconciliationStatus !== "not_required").length,
        verified: view.sources.filter((line) => line.kind === "ai" && line.reconciliationStatus === "verified").length,
      },
      lines: view.lines.filter((line) => line.kind !== "seat" || line.sourceKey === null)
        .map((line) => ({ kind: line.kind, description: line.description,
        amountSatang: line.amountSatang, measuredQuantity: line.measuredQuantity })),
      totalSatang: view.totalSatang,
      invoice: view.invoice,
      state: view.state,
    } }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
