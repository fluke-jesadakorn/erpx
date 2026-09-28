import { requireManageErpxContext } from "@/server/authorization";
import { readSeatForecastBaseline } from "@/server/billing-forecast";
import { ApiError, errorResponse } from "@/server/http";
import { runtimeEnv } from "@/server/runtime";
import type { PostgresDatabase } from "@/server/runtime/postgres";

export async function GET(request: Request) {
  try {
    const { platform } = await requireManageErpxContext();
    if (platform.role !== "platform_admin" && platform.role !== "platform_auditor")
      throw new ApiError(403, "FORBIDDEN", "Platform billing permission required.");
    const holdingId = new URL(request.url).searchParams.get("holdingId");
    if (!holdingId || holdingId === "h003")
      throw new ApiError(422, "HOLDING_REQUIRED", "Select a Customer Holding.");
    const db = (await runtimeEnv()).CONTROL_DB as PostgresDatabase;
    const binding = await db.pool.query(
      "SELECT 1 FROM holding_bindings WHERE holding_id=$1 AND holding_id<>'h003'", [holdingId]);
    if (!binding.rowCount) throw new ApiError(404, "NOT_FOUND", "Customer Holding not found.");
    return Response.json({ data: await readSeatForecastBaseline(db, holdingId) },
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
