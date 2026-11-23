import "server-only";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { nativeBillingOwnerQuery } from "@erpx/server-billing/adapters/native-billing-owner-write";

type FinanceProjection = {
  invoice_satang: string;
  receivable_satang: string;
  received_satang: string;
  reversed_satang: string;
  credited_satang: string;
  stripe_status: string;
  synced_at: string;
};

/** Owner-authored finance projection read; the native Billing port owns the SQL. */
export async function appBillingFinanceProjection(db: PostgresDatabase, invoiceId: string, holdingId: string): Promise<FinanceProjection | undefined> {
  const { rows } = (await nativeBillingOwnerQuery(db.pool, "app_finance_projection", [invoiceId, holdingId])) as { rows: FinanceProjection[] };
  return rows[0];
}
