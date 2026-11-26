import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {headers} from "next/headers";
import {customerHoldingBindingExists} from "@erpx/server-runtime/public/app-native-sources";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {createNativeBillingClient,nativeBillingConfiguration,nativeBillingFailure,type NativeBillingPageContext} from "@erpx/server-billing/public/native-billing";
import {ApiError, errorResponse} from "@erpx/server-runtime/public/http";
import {runtimeEnv} from "../../../../../../../../../packages/server-runtime/src/public/runtime";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";

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
    const raw = process.env.ERPX_NATIVE_BILLING_CONFIG;
    if (raw === undefined)
      throw new ApiError(503, "NATIVE_BILLING_UNAVAILABLE", "The native Billing forecast is not configured.");
    const client = createNativeBillingClient(nativeBillingConfiguration(raw));
    const holding = client.holding(holdingId);
    if (!holding)
      throw new ApiError(503, "NATIVE_BILLING_UNAVAILABLE", "The native Billing Holding target is not configured.");
    const actor: NativeBillingPageContext = {
      userId: platform.userId,
      holdingId,
      holdingSlug: holding.holdingSlug,
      platform: true,
      role: platform.role,
      assignmentId: platform.assignmentId,
    };
    const data = await client.forecast(await headers(), actor, { holdingId, holdingSlug: holding.holdingSlug });
    return Response.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    const mapped = nativeBillingFailure(error);
    return errorResponse(mapped ? new ApiError(mapped.status, mapped.code, mapped.message) : error);
  }
}
