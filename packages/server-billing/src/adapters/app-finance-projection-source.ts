import "server-only";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
export async function appBillingFinanceProjection(db: PostgresDatabase, invoiceId: string, holdingId: string) {
  return (await db.pool.query<{ invoice_satang: string; receivable_satang: string; received_satang: string; reversed_satang: string; credited_satang: string; stripe_status: string; synced_at: string }>(`SELECT invoice_satang,receivable_satang,received_satang,reversed_satang,
        credited_satang,stripe_status,synced_at FROM billing_test_finance_projection
        WHERE billing_invoice_id=$1 AND customer_holding_id=$2`, [invoiceId, holdingId])).rows[0];
}
