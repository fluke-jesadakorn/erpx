import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireManageErpxContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { activeHoldingBindings, runtimeEnv } from "../../../../../../../../../packages/server-runtime/src/public/runtime";
import { usageMonth } from "@erpx/server-commercial/public/usage";
import { readPlatformUsageProjection } from "@erpx/server-commercial/public/usage-projection";

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
    return Response.json(canonicalDestinationLinks({ data: {
      month, scope: "platform", holdings: report.holdings,
      accounts: all.map(binding => ({ holdingId: binding.id, holdingName: binding.displayName })),
      items: report.items,
      unavailable: report.unavailable, measured: true, coverage: "partial",
    } }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
