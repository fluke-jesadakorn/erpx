import "server-only";
/** Closed retained identities, declared by POS. Source labels in Finance input
 * are not evidence that a journal/invoice belongs to a POS transaction. */
export const posFinanceRecordSources = {
  invoice: "SELECT company_id,invoice_id id FROM pos_sales WHERE invoice_id IS NOT NULL",
  journal: "SELECT company_id,journal_id id FROM pos_postings WHERE journal_id IS NOT NULL UNION SELECT company_id,journal_id FROM pos_settlements WHERE journal_id IS NOT NULL",
  payment: "SELECT company_id,finance_payment_id id FROM pos_payments WHERE finance_payment_id IS NOT NULL",
} as const;

/** Query-free source leases for retained Finance document identities. */
export function posFinanceRecordSourceReadLocks(database:import('../../runtime/public/runtime/types').SqlDatabase){return database.prepare('LOCK TABLE pos_sales,pos_postings,pos_settlements,pos_payments IN SHARE ROW EXCLUSIVE MODE');}

/** Opaque native relationship headers, separate from the existing identity
 * declarations. Finance compares them only inside its leased read unit. */
export const posFinanceRecordSourceImages={
 journal:"SELECT company_id,journal_id id,to_jsonb(source) source_image FROM pos_postings source UNION ALL SELECT company_id,journal_id,to_jsonb(source) FROM pos_settlements source WHERE journal_id IS NOT NULL",
 payment:"SELECT company_id,finance_payment_id id,to_jsonb(source) source_image FROM pos_payments source WHERE finance_payment_id IS NOT NULL",
} as const;
