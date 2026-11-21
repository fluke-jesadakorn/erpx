import { commerceFinalOwnerProjectionSources as procurementProjectionSources } from "@erpx/server-procurement/public/commerce-final-owner-projection-sources";
import "server-only";
import type { SqlDatabase, SqlResult, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { nativeOwnerStatement } from "@erpx/server-commerce/adapters/native-owner-write";

/** Native owner write stamped inside the Procurement caller's batch. The
 * returned object carries no SQL: its run() executes the write in Rust. */
export function stageSupplierBillReceiptVersion(db: SqlDatabase, id: string): SqlStatement {
  return nativeOwnerStatement('supplier.fulfillment-version', [id], id, 'system', `supplier:fulfillment-version:${id}`, id);
}

/** Resolve the matched fulfillment ids on the caller's owned snapshot, then
 * write the invoice links through the native owner axum command. The read stays
 * in TS; the write is Rust-owned with no SQL mirror. */
export function stageSupplierBillReceiptLinks(db: SqlDatabase, matchId: string, invoiceId: string | null): SqlStatement {
  const statement: SqlStatement = {
    bind() { return statement; },
    async run<T>(): Promise<SqlResult<T>> {
      const rows = (await db.prepare(`SELECT fulfillment_id FROM (${procurementProjectionSources.procurement_bill_match_receipts}) procurement_bill_match_receipts WHERE match_id=?${invoiceId === null ? '' : ' AND active=TRUE'}`).bind(matchId).all<{ fulfillment_id: string }>()).results;
      const ids = rows.map(row => String(row.fulfillment_id));
      await nativeOwnerStatement('supplier.fulfillment-links', [invoiceId, JSON.stringify(ids)], matchId, 'system', `supplier:fulfillment-links:${matchId}`).run();
      return { results: [], success: true, meta: { changes: ids.length, duration: 0 } };
    },
    async first<T>(): Promise<T | null> { throw new Error('COMMERCE_OWNER_WRITE_READ_UNSUPPORTED'); },
    async all<T>(): Promise<SqlResult<T>> { throw new Error('COMMERCE_OWNER_WRITE_READ_UNSUPPORTED'); },
    async raw<T>(): Promise<T[]> { throw new Error('COMMERCE_OWNER_WRITE_READ_UNSUPPORTED'); },
  };
  return statement;
}
