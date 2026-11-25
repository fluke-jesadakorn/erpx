import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { PersistedInvoiceLine } from "@erpx/server-finance/domain/invoice-line-revision";
import {
  nativeCurrentInvoiceLines,
  type Lane3Actor,
} from "./native-lane-3/client";

/**
 * Native lane 3 cutover. The owner axum route executes the company-scoped
 * statement and projects every returned field; this adapter is a strict,
 * fail-closed transport and emits no SQL. The `db` argument is retained for
 * call-site parity only. The Holding is selected from the caller's WorkContext.
 */
export async function currentInvoiceLines(
  _db: SqlDatabase,
  actor: Lane3Actor,
  invoiceId: string,
  revision: number,
): Promise<PersistedInvoiceLine[]> {
  void _db;
  return nativeCurrentInvoiceLines(actor, invoiceId, revision);
}
