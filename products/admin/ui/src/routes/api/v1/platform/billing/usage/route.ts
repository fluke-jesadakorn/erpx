import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireManageErpxContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import { activeHoldingBindings, runtimeEnv } from "../../../../../../../../../../platform/runtime/server/src/public/runtime";
import { usageMonth } from "@erpx/commercial-server/public/usage";
import { readPlatformUsageProjection } from "@erpx/commercial-server/public/usage-projection";

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
