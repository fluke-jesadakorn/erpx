import {appBillingFinanceProjection} from "@/server/modules/billing/public/app-finance-projection-source";
import {customerHoldingBindingExists} from "@/server/modules/runtime/public/app-native-sources";
import { billingMonth } from "@/lib/billing";
import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";
import { readTargetBillingPeriod } from "@/server/modules/billing/public/billing-target";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";

export async function GET(request: Request) {
  try {
    const { platform } = await requireManageErpxContext();
    if (platform.role !== "platform_admin" && platform.role !== "platform_auditor")
      throw new ApiError(403, "FORBIDDEN", "Platform billing permission required.");
    const query = new URL(request.url).searchParams;
    const holdingId = query.get("holdingId");
    if (!holdingId || holdingId === "h003")
      throw new ApiError(422, "HOLDING_REQUIRED", "Select a Customer Holding.");
    const month = query.get("month") ?? billingMonth(Date.now());
    if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(month))
      throw new ApiError(422, "INVALID_MONTH", "Use a month in YYYY-MM format.");
    const db = (await runtimeEnv()).CONTROL_DB as PostgresDatabase;
    const binding = await customerHoldingBindingExists(db,holdingId);
    if (!binding) throw new ApiError(404, "NOT_FOUND", "Customer Holding not found.");
    const view = await readTargetBillingPeriod(db, holdingId, month);
    const receipt = view.invoice ? await appBillingFinanceProjection(db,view.invoice.id,holdingId) : null;
    const financeProjection = receipt ? {
      invoiceSatang: Number(receipt.invoice_satang), receivableSatang: Number(receipt.receivable_satang),
      receivedSatang: Number(receipt.received_satang), reversedSatang: Number(receipt.reversed_satang),
      creditedSatang: Number(receipt.credited_satang), stripeStatus: receipt.stripe_status,
      syncedAt: Number(receipt.synced_at),
    } : null;
    return Response.json({ data: { ...view, financeProjection } },
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
