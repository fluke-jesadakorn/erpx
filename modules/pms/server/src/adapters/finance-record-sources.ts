import "server-only";
import { pmsJournalIdentityUnionSql } from "./pms-read-tail-r39-sql";
import { pmsPaymentUnionSql } from "./pms-read-tail-r44-sql";
/** Closed Finance-payment identity union shared by the Hotel document reader and
 * package settlement sources. Text only: callers keep their own scope/authority
 * guards, binds and transaction. The sole variant is the optional `id` output
 * alias on the document finance_id column; no table, predicate or SQL is caller-supplied. */
export function pmsPaymentIdentityUnionSql(aliasDocumentId: boolean): string {
  return pmsPaymentUnionSql('identity', aliasDocumentId);
}
/** Only actual Hotel document/posting identities reach the Finance reader. */
export const pmsFinanceRecordSources = {
  invoice: "SELECT p.company_id,d.finance_id id FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id WHERE d.kind='invoice' AND d.finance_id IS NOT NULL",
  journal: pmsJournalIdentityUnionSql('identity'),
  payment: pmsPaymentIdentityUnionSql(true),
} as const;

/** Query-free source leases for retained Finance document identities. */
export function pmsFinanceRecordSourceReadLocks(database:import("@erpx/platform-runtime-server/public/runtime/types").SqlDatabase){return database.prepare('LOCK TABLE pms_properties,pms_documents,pms_postings,pms_import_batches,pms_transfer_requests,pms_import_balances IN SHARE ROW EXCLUSIVE MODE');}

/** Full retained native relationship headers remain server-only. */
export const pmsFinanceRecordSourceImages={
 journal: pmsJournalIdentityUnionSql('sourceImage'),
 payment:pmsPaymentUnionSql('sourceImage'),
} as const;
