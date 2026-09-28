import { requireManageErpxContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import { activeHoldingBindings, runtimeEnv } from "@/server/runtime";
import { usageMonth } from "@/server/usage";
import { readPlatformUsageProjection } from "@/server/usage-projection";

export async function GET(request: Request) {
  try {
    const { platform } = await requireManageErpxContext();
    if (platform.role !== "platform_admin" && platform.role !== "platform_auditor")
      throw new ApiError(403, "FORBIDDEN", "Platform billing permission required.");
    const query = new URL(request.url).searchParams;
    const month = usageMonth(query.get("month")).month;
    const selectedId = query.get("holdingId");
    const all = (await activeHoldingBindings()).filter(binding => binding.id !== "h003");
    if (selectedId && !all.some(binding => binding.id === selectedId))
      throw new ApiError(404, "NOT_FOUND", "Customer Holding not found.");
    const { CONTROL_DB } = await runtimeEnv();
    const report = await readPlatformUsageProjection(CONTROL_DB, all, month, selectedId);
    return Response.json({ data: {
      month, scope: "platform", holdings: report.holdings,
      accounts: all.map(binding => ({ holdingId: binding.id, holdingName: binding.displayName })),
      items: report.items,
      unavailable: report.unavailable, measured: true, coverage: "partial",
    } }, { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
