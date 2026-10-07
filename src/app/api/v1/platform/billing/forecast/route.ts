import {customerHoldingBindingExists} from "@/server/modules/runtime/public/app-native-sources";
import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";
import { readSeatForecastBaseline } from "@/server/modules/billing/public/billing-forecast";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";

export async function GET(request: Request) {
  try {
    const { platform } = await requireManageErpxContext();
    if (platform.role !== "platform_admin" && platform.role !== "platform_auditor")
      throw new ApiError(403, "FORBIDDEN", "Platform billing permission required.");
    const holdingId = new URL(request.url).searchParams.get("holdingId");
    if (!holdingId || holdingId === "h003")
      throw new ApiError(422, "HOLDING_REQUIRED", "Select a Customer Holding.");
    const db = (await runtimeEnv()).CONTROL_DB as PostgresDatabase;
    const binding = await customerHoldingBindingExists(db,holdingId);
    if (!binding) throw new ApiError(404, "NOT_FOUND", "Customer Holding not found.");
    return Response.json({ data: await readSeatForecastBaseline(db, holdingId) },
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
