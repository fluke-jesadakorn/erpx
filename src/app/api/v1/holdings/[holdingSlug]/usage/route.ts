import { requireWorkContext, requestedHoldingScope } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { readHoldingUsage, usageMonth } from "@/server/modules/commercial/public/usage";
import { hasPermission } from "@/lib/identity_access/permissions";

export async function GET(request: Request, context: { params: Promise<{ holdingSlug: string }> }) {
  try {
    const { holdingSlug } = await context.params;
    const requested = await requestedHoldingScope(holdingSlug);
    if (requested.kind !== "holding" && requested.kind !== "branch")
      throw new ApiError(403, "FORBIDDEN", "Usage is available in Holding and Branch workspaces.");
    const actor = await requireWorkContext(requested);
    const owner = actor.role === "holding_owner" && actor.scope.kind === "holding";
    const branchId = actor.scope.kind === "branch" ? actor.scope.branchId : null;
    const branchManager = actor.role === "operations_manager" && actor.assignmentScope === "branch" && branchId !== null &&
      actor.availableAssignments?.some(assignment => assignment.id === actor.assignmentId && assignment.scopeNodeId === branchId);
    if ((!owner && !branchManager) || !hasPermission(actor, "billing.usage.read"))
      throw new ApiError(403, "FORBIDDEN", "Usage is unavailable in your current work role.");
    const month = usageMonth(new URL(request.url).searchParams.get("month")).month;
    const { holding, items } = await readHoldingUsage({ id: actor.holdingId, slug: actor.holdingSlug, displayName: actor.holdingName }, month,
      branchManager ? branchId : null);
    const usageOnly = (metrics: typeof holding.totals) => ({
      ai: metrics.ai,
      storage: { bytes: metrics.storage.bytes, fileCount: metrics.storage.fileCount },
    });
    const visibleHolding = owner ? holding : {
      ...holding, sandboxStoragePreview: undefined, seatUsage: undefined,
      meterCoverage: { ai: holding.meterCoverage.ai, storage: holding.meterCoverage.storage,
        unreconciledOriginals: holding.meterCoverage.unreconciledOriginals },
      totals: usageOnly(holding.totals),
      companies: holding.companies.map(company => ({
        ...company, totals: usageOnly(company.totals), unassigned: usageOnly(company.unassigned),
        branches: company.branches.map(branch => ({ ...branch, totals: usageOnly(branch.totals) })),
      })),
    };
    const visibleItems = owner ? items : items.map(item => ({
      id: item.id, holdingId: item.holdingId,
      companyId: item.companyId, companyName: item.companyName,
      branchId: item.branchId, branchName: item.branchName,
      type: item.type, quantity: item.quantity, unit: item.unit,
      period: item.period, source: item.source,
      inputTokens: item.inputTokens, outputTokens: item.outputTokens,
      occurredAt: item.occurredAt, requestStatus: item.requestStatus,
    }));
    return Response.json({ data: { month, scope: owner ? "holding" : "branch", holdings: [visibleHolding], items: visibleItems, measured: true, coverage: "partial" } },
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
