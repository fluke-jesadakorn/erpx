import { billingMonth } from "@/lib/billing";
import { requireManageErpxContext } from "@/server/authorization";
import { readTargetBillingPeriod } from "@/server/billing-target";
import { ApiError, errorResponse } from "@/server/http";
import { runtimeEnv } from "@/server/runtime";
import type { PostgresDatabase } from "@/server/runtime/postgres";

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
    const binding = await db.pool.query("SELECT 1 FROM holding_bindings WHERE holding_id=$1 AND holding_id<>'h003'", [holdingId]);
    if (!binding.rowCount) throw new ApiError(404, "NOT_FOUND", "Customer Holding not found.");
    const view = await readTargetBillingPeriod(db, holdingId, month);
    const receipt = view.invoice ? (await db.pool.query<{
      invoice_satang: string; receivable_satang: string; received_satang: string;
      reversed_satang: string; credited_satang: string; stripe_status: string; synced_at: string;
    }>(`SELECT invoice_satang,receivable_satang,received_satang,reversed_satang,
        credited_satang,stripe_status,synced_at FROM billing_test_finance_projection
        WHERE billing_invoice_id=$1 AND customer_holding_id=$2`, [view.invoice.id, holdingId])).rows[0] : null;
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
